//! Native behavior is never converted between games. A failed destination
//! native instance commits no duplicate source world execution, and a fresh
//! runtime recovers the same global entity from the durable SQL checkpoint.
use std::collections::BTreeMap;
use std::time::{Duration,SystemTime};
use oasis_contracts::{
    ClockStep, ContractError, ContractResult, DefinitionRef, EntityView,
    Id, InputIntent, ModuleDescriptor, NativeHandle, NativeModule,
    Revision, Snapshot, StepOutput, TypeRef, TypedValue, Value, WorldPort,
};
use oasis_runtime::universe::{
    PersistedPlacement, Principal, SessionAuthority, Universe,
};
use oasis_service::{LiveTravelRequest, LiveTravelService, RecoveryService};
use oasis_store::{DurableWorldStore, PostgresStore};

struct NativeFixture {
    context:Id,
    reject_new:bool,
    next:u64,
    snapshots:BTreeMap<u64,Snapshot>,
}
impl NativeFixture {
    fn new(context:Id,reject_new:bool)->Self {
        Self{context,reject_new,next:0,snapshots:BTreeMap::new()}
    }
}
impl NativeModule for NativeFixture {
    fn descriptor(&self)->ModuleDescriptor {
        ModuleDescriptor{
            engine_id:Id(0x3),module_id:Id(0x5),
            contract_major:0,contract_minor:1,
            exported_interfaces:vec![],required_interfaces:vec![],
        }
    }
    fn instantiate(&mut self,entity:&EntityView,checkpoint:Option<&Snapshot>)
        ->ContractResult<NativeHandle>{
        if self.reject_new {
            return Err(ContractError::InvalidData("injected engine failure".into()));
        }
        self.next+=1;
        let initial=Snapshot{
            entity_id:entity.id,revision:Revision(4),
            binary_artifact:None,
            state:vec![TypedValue{
                type_ref:TypeRef{
                    namespace:"native".into(),name:"original-item-state".into(),version:1,
                },
                data:Value::Bytes(vec![0,1,2,255]),
            }],
        };
        self.snapshots.insert(self.next,checkpoint.cloned().unwrap_or(initial));
        Ok(NativeHandle{context_id:self.context,native_slot:self.next})
    }
    fn step(&mut self,_clock:ClockStep,_input:&[InputIntent],
        _world:&mut dyn WorldPort)->ContractResult<StepOutput>{
        Ok(StepOutput{state_changes:self.snapshots.values().cloned().collect(),
            interactions:vec![],emitted_events:vec![]})
    }
    fn snapshot(&self,handle:NativeHandle)->ContractResult<Snapshot>{
        self.snapshots.get(&handle.native_slot).cloned()
            .ok_or(ContractError::NotFound(self.context))
    }
    fn restore(&mut self,handle:NativeHandle,snapshot:&Snapshot)
        ->ContractResult<()>{
        *self.snapshots.get_mut(&handle.native_slot)
            .ok_or(ContractError::NotFound(self.context))?=snapshot.clone();
        Ok(())
    }
    fn remove(&mut self,handle:NativeHandle)->ContractResult<()>{
        self.snapshots.remove(&handle.native_slot)
            .ok_or(ContractError::NotFound(self.context))?;
        Ok(())
    }
}
struct TestSession;
impl SessionAuthority for TestSession {
    fn authenticate(&self,credential:&str)->ContractResult<Principal>{
        if credential!="authenticated" {
            return Err(ContractError::InvalidData("unauthorized".into()));
        }
        Ok(Principal{
            user_id:Id(0x2),player_id:Id(0x9),character_id:Id(0x7),
            expires_at:SystemTime::now()+Duration::from_secs(600),
        })
    }
}
fn request(
    destination_world:Id,destination_frame:Id,
    transaction_id:Id,presence_id:Id,snapshot_id:Id,key:&str
)->LiveTravelRequest{
    LiveTravelRequest{
        session_id:Id(0x21),
        destination_world_id:destination_world,
        destination_frame_id:destination_frame,
        destination_presence_id:presence_id,
        transaction_id,idempotency_key:key.into(),
        snapshot_id,snapshot_type_id:Id(0x1),event_type_id:Id(0x1),
    }
}
fn universe(first_reject:bool,second_reject:bool)->Universe {
    let mut world=Universe::new();
    world.add_world(Id(0x13)).unwrap();
    world.add_world(Id(0x23)).unwrap();
    world.add_module(Id(0x13),Id(0x20),
        Box::new(NativeFixture::new(Id(0x20),first_reject))).unwrap();
    world.add_module(Id(0x23),Id(0x25),
        Box::new(NativeFixture::new(Id(0x25),second_reject))).unwrap();
    world
}

#[test]
fn failed_native_destination_is_quarantined_then_recovered_without_duplicating() {
    let url=std::env::var("DATABASE_URL").expect("fresh dedicated OASIS CI database");
    let store=PostgresStore::connect(&url).unwrap();
    let entity=store.load_entity(Id(0x7)).unwrap().unwrap();
    assert_eq!(entity.origin_module,Some(Id(0x5)));
    let current=store.active_presence(Id(0x7)).unwrap().unwrap();
    assert_eq!(current.presence_id,Id(0x15));
    assert_eq!(current.authority_epoch,1);
    assert!(store.latest_state(Id(0x7)).unwrap().is_none());

    // Both world adapters register the SAME origin module independently.
    // The destination intentionally fails to instantiate its native logic.
    let mut running=universe(false,true);
    running.recover_entity(entity,None,Some(PersistedPlacement{
        world_instance_id:Id(0x13),
        authority_context_id:Id(0x20),
        authority_epoch:1,
    })).unwrap();
    let ticket=running.connect(&TestSession,"authenticated").unwrap();
    let service=LiveTravelService::new(&store);
    let out=request(Id(0x23),Id(0x24),Id(0x70),Id(0x71),Id(0x72),"native-transfer");
    assert!(service.transfer(&mut running,ticket,&out).is_err());

    // SQL is ALREADY committed. No local second copy may continue running.
    let durable=store.active_presence(Id(0x7)).unwrap().unwrap();
    assert_eq!(durable.world_instance_id,Id(0x23));
    assert_eq!(durable.authority_context_id,Id(0x25));
    assert_eq!(durable.authority_epoch,2);
    assert_eq!(running.presence(Id(0x7)),None);
    assert!(running.is_world_quarantined(Id(0x23)));
    assert!(matches!(running.simulation_leases(Id(0x23)),
        Err(ContractError::StaleAuthority)));
    let (version,native)=store.latest_state(Id(0x7)).unwrap().unwrap();
    assert_eq!(version,1);
    assert_eq!(native.revision,Revision(4));
    assert_eq!(native.state[0].data,Value::Bytes(vec![0,1,2,255]));

    // A new world-server process recovers that *same* native item/character
    // from the committed checkpoint; no pairwise compatibility code.
    let mut resumed=universe(false,false);
    let recovery=RecoveryService::new(&store);
    let state=recovery.restore_session(&mut resumed,Id(0x21)).unwrap();
    assert_eq!(state.native_revision,Some(4));
    assert_eq!(state.authority_epoch,2);
    assert_eq!(state.world_instance_id,Id(0x23));
    let restored_ticket=resumed.connect(&TestSession,"authenticated").unwrap();
    assert_eq!(resumed.observe(restored_ticket).unwrap()[0].state,native.state);

    // Travel back now succeeds: source and destination both use the same
    // generic module contract, identity and state persist unchanged.
    let back=request(Id(0x13),Id(0x14),Id(0x74),Id(0x75),Id(0x76),"return-transfer");
    let result=service.transfer(&mut resumed,restored_ticket,&back).unwrap();
    assert!(result.applied);
    assert_eq!(result.authority_epoch,3);
    assert_eq!(result.snapshot_revision,2);
    assert_eq!(resumed.presence(Id(0x7)),Some(Id(0x13)));
    assert_eq!(store.active_presence(Id(0x7)).unwrap().unwrap().world_instance_id,Id(0x13));
    assert_eq!(store.latest_state(Id(0x7)).unwrap().unwrap().1.state,native.state);
    assert_eq!(store.load_entity(Id(0x7)).unwrap().unwrap().origin_module,Some(Id(0x5)));

    // SQL failures cannot silently remap the source entity to another world.
    let rejected=request(Id(0x23),Id(0x14),Id(0x77),Id(0x78),Id(0x79),"bad-frame");
    assert!(service.transfer(&mut resumed,restored_ticket,&rejected).is_err());
    assert_eq!(store.active_presence(Id(0x7)).unwrap().unwrap().world_instance_id,Id(0x13));
    assert_eq!(resumed.presence(Id(0x7)),None); // fail closed after ambiguity
}
