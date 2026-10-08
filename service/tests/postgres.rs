//! Runs on the real, disposable PostgreSQL CI fixture after the storage
//! travel tests, validating native-world recovery of the committed snapshot.
use std::collections::BTreeMap;
use std::sync::{Arc,Mutex};
use std::time::{Duration,Instant};

use oasis_contracts::{
    ClockStep, ContractError, ContractResult, EntityView, Id, InputIntent,
    ModuleDescriptor, NativeHandle, NativeModule, Snapshot, StepOutput, WorldPort,
};
use oasis_runtime::universe::{Principal, SessionAuthority, Universe};
use oasis_runtime::scheduler::WorldRunner;
use oasis_service::{PostgresTickAuthorizer,RecoveryError, RecoveryService};
use oasis_store::{DurableWorldStore, PostgresStore};

struct NativeFixture {
    context: Id, next: u64, snapshots: BTreeMap<u64, Snapshot>,
}
impl NativeFixture {
    fn new(context:Id) -> Self {
        Self{context,next:0,snapshots:BTreeMap::new()}
    }
}
impl NativeModule for NativeFixture {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor {
            engine_id:Id(0x3),module_id:Id(0x5),
            contract_major:0,contract_minor:1,
            exported_interfaces:vec![],required_interfaces:vec![],
        }
    }
    fn instantiate(&mut self, entity:&EntityView, saved:Option<&Snapshot>)
        -> ContractResult<NativeHandle> {
        let state=saved.ok_or_else(||
            ContractError::InvalidData("native state was not recovered".into()))?;
        if state.entity_id != entity.id {
            return Err(ContractError::InvalidData("wrong snapshot".into()));
        }
        self.next+=1;
        self.snapshots.insert(self.next,state.clone());
        Ok(NativeHandle{context_id:self.context,native_slot:self.next})
    }
    fn step(&mut self, _clock:ClockStep, _input:&[InputIntent],
            _world:&mut dyn WorldPort) -> ContractResult<StepOutput> {
        Ok(StepOutput{
            state_changes:self.snapshots.values().cloned().collect(),
            interactions:vec![],emitted_events:vec![],
        })
    }
    fn snapshot(&self, handle:NativeHandle) -> ContractResult<Snapshot> {
        self.snapshots.get(&handle.native_slot).cloned()
            .ok_or(ContractError::NotFound(self.context))
    }
    fn restore(&mut self, handle:NativeHandle, state:&Snapshot)
        -> ContractResult<()> {
        let existing=self.snapshots.get_mut(&handle.native_slot)
            .ok_or(ContractError::NotFound(self.context))?;
        *existing=state.clone();
        Ok(())
    }
    fn remove(&mut self, handle:NativeHandle) -> ContractResult<()> {
        self.snapshots.remove(&handle.native_slot)
            .ok_or(ContractError::NotFound(self.context))?;
        Ok(())
    }
}

struct VerifiedSession<'a>(&'a PostgresStore);
impl SessionAuthority for VerifiedSession<'_> {
    fn authenticate(&self, credential:&str) -> ContractResult<Principal> {
        if credential!="test-authenticated-session" {
            return Err(ContractError::InvalidData("unauthorized".into()));
        }
        let session=self.0.load_session(Id(0x21))
            .map_err(|err|ContractError::Internal(err.to_string()))?
            .ok_or_else(||ContractError::InvalidData("session expired".into()))?;
        Ok(Principal {
            user_id:session.user_id,
            player_id:session.player_id,
            character_id:session.character_id,
            expires_at:session.expires_at,
        })
    }
}

#[test]
fn committed_native_state_is_restored_to_original_engine_module() {
    let url=std::env::var("DATABASE_URL").expect("PostgreSQL CI DATABASE_URL");
    let store=PostgresStore::connect(&url).unwrap();
    let presence=store.active_presence(Id(0x7)).unwrap().unwrap();

    let mut universe=Universe::new();
    universe.add_world(presence.world_instance_id).unwrap();
    universe.add_module(
        presence.world_instance_id, presence.authority_context_id,
        Box::new(NativeFixture::new(presence.authority_context_id)),
    ).unwrap();

    let loader=RecoveryService::new(&store);
    assert!(matches!(
        loader.restore_session(&mut universe,Id(0x999)),
        Err(RecoveryError::InvalidSession),
    ));
    let receipt=loader.restore_session(&mut universe,Id(0x21)).unwrap();
    assert_eq!(receipt.character_id,Id(0x7));
    assert_eq!(receipt.world_instance_id,presence.world_instance_id);
    assert_eq!(receipt.authority_epoch,presence.authority_epoch);
    assert_eq!(receipt.durable_revision,Some(2));
    assert_eq!(receipt.native_revision,Some(8));

    let ticket=universe.connect(
        &VerifiedSession(&store),"test-authenticated-session",
    ).unwrap();
    let snaps=universe.observe(ticket).unwrap();
    assert_eq!(snaps.len(),1);
    assert_eq!(snaps[0].entity_id,Id(0x7));
    assert_eq!(snaps[0].revision.0,8);
    let (_,persisted)=store.latest_state(Id(0x7)).unwrap().unwrap();
    assert_eq!(snaps[0].state,persisted.state);

    // A second loader cannot duplicate the persistent character in memory.
    assert!(matches!(
        loader.restore_session(&mut universe,Id(0x21)),
        Err(RecoveryError::Runtime(_)),
    ));

    // Simulate the first world-server process terminating. A new process
    // registers the SAME native source module with a NEW execution context.
    let mut replacement=Universe::new();
    replacement.add_world(presence.world_instance_id).unwrap();
    let next_context=Id(0x60);
    replacement.add_module(
        presence.world_instance_id,next_context,
        Box::new(NativeFixture::new(next_context)),
    ).unwrap();
    assert!(matches!(
        loader.reclaim_expired_session(&mut replacement,Id(0x21),next_context),
        Err(RecoveryError::LiveAuthority(_)),
    ));
    // Expire the prior lease (equivalent to a failed heartbeat), then a
    // genuinely new server context can claim it, with a higher epoch.
    let mut administrator=postgres::Client::connect(&url,postgres::NoTls).unwrap();
    administrator.batch_execute(
        "INSERT INTO execution_contexts(id,world_instance_id,module_id,status)
         VALUES('00000000-0000-0000-0000-000000000060',
                '00000000-0000-0000-0000-000000000013',
                '00000000-0000-0000-0000-000000000005','running');
         UPDATE authority_leases SET expires_at=now()-INTERVAL '1 second'
          WHERE resource_key='character:00000000-0000-0000-0000-000000000007';"
    ).unwrap();
    let rebooted=loader.reclaim_expired_session(
        &mut replacement,Id(0x21),next_context,
    ).unwrap();
    assert_eq!(rebooted.world_instance_id,presence.world_instance_id);
    assert_eq!(rebooted.authority_epoch,presence.authority_epoch+1);
    assert_eq!(rebooted.native_revision,Some(8));
    assert_eq!(rebooted.durable_revision,Some(2));
    assert_eq!(store.active_presence(Id(0x7)).unwrap().unwrap()
        .authority_context_id,next_context);
    let recovered_ticket=replacement.connect(
        &VerifiedSession(&store),"test-authenticated-session",
    ).unwrap();
    let recovered_snapshots=replacement.observe(recovered_ticket).unwrap();
    assert_eq!(recovered_snapshots[0].state,persisted.state);

    // The next server can simulate only as long as its PostgreSQL lease is
    // current. Revocation between ticks stops further native calls.
    let gate=Arc::new(PostgresTickAuthorizer::new(
        Arc::new(PostgresStore::connect(&url).unwrap()),
    ));
    let guarded=Arc::new(Mutex::new(replacement));
    let runner=WorldRunner::start(
        Arc::clone(&guarded),presence.world_instance_id,
        Duration::from_millis(5),gate,
    ).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    assert!(runner.last_error().is_none());
    administrator.batch_execute(
        "UPDATE authority_leases SET expires_at=now()-INTERVAL '1 second'
          WHERE resource_key='character:00000000-0000-0000-0000-000000000007';"
    ).unwrap();
    let deadline=Instant::now()+Duration::from_secs(3);
    while runner.last_error().is_none() && Instant::now()<deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(matches!(
        runner.last_error(),Some(ContractError::StaleAuthority),
    ));
}
