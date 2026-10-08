//! Runs on the real, disposable PostgreSQL CI fixture after the storage
//! travel tests, validating native-world recovery of the committed snapshot.
use std::collections::BTreeMap;

use oasis_contracts::{
    ClockStep, ContractError, ContractResult, EntityView, Id, InputIntent,
    ModuleDescriptor, NativeHandle, NativeModule, Snapshot, StepOutput, WorldPort,
};
use oasis_runtime::universe::{Principal, SessionAuthority, Universe};
use oasis_service::{RecoveryError, RecoveryService};
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
}
