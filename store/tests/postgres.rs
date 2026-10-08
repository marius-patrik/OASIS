//! Actual PostgreSQL integration: no mocks, authoritative session and
//! original-game snapshot survive multiple round trips between world servers.
use std::env;
use oasis_contracts::{Id, Revision, Snapshot, TypeRef, TypedValue, Value};
use oasis_store::{
    DurableWorldStore, PostgresStore, TravelCommand,
};

// SQL fixtures use UUID suffixes written in hexadecimal, not decimal.
const fn id(n: u128) -> Id { Id(n) }
fn state(revision: u64) -> Snapshot {
    Snapshot {
        entity_id:id(0x7),revision:Revision(revision),binary_artifact:Some(id(0x4)),
        state:vec![TypedValue{
            type_ref:TypeRef{
                namespace:"source.game".into(),name:"equipment".into(),version:2,
            },
            data:Value::Map(std::collections::BTreeMap::from([
                ("damage".into(),Value::UInt(99)),
                ("alive".into(),Value::Bool(true)),
                ("nativeRevision".into(),Value::UInt(revision)),
            ])),
        }],
    }
}
fn command(
    transaction:u128, key:&str, previous:u128, next:u128,
    world:u128, frame:u128, context:u128, epoch:u64,
    snapshot_id:u128, previous_revision:u64, native_revision:u64,
) -> TravelCommand {
    TravelCommand {
        transaction_id:id(transaction),
        idempotency_key:key.to_string(),
        session_id:id(0x21),
        character_id:id(0x7),
        expected_presence_id:Some(id(previous)),
        destination_presence_id:id(next),
        destination_world_id:id(world),
        destination_frame_id:id(frame),
        destination_context_id:id(context),
        expected_authority_epoch:epoch,
        event_type_id:id(0x1),
        snapshot_id:id(snapshot_id),
        snapshot_type_id:id(0x1),
        expected_snapshot_revision:previous_revision,
        native_state:state(native_revision),
    }
}

#[test]
fn durable_native_state_travels_between_worlds_without_replacing_the_engine() {
    let url=env::var("DATABASE_URL")
        .expect("CI must provide PostgreSQL DATABASE_URL");
    let store=PostgresStore::connect(&url).unwrap();
    let session=store.load_session(id(0x21)).unwrap().unwrap();
    assert_eq!(session.character_id,id(0x7));
    assert_eq!(session.player_id,id(0x9));
    let entity=store.load_entity(id(0x7)).unwrap().unwrap();
    assert_eq!(entity.origin_module,Some(id(0x5)));
    assert!(!entity.components.is_empty());
    assert_eq!(store.active_presence(id(0x7)).unwrap().unwrap().presence_id,id(0x15));
    assert!(store.latest_state(id(0x7)).unwrap().is_none());

    let outbound=command(0x40,"rust-pg-outbound",0x15,0x41,0x23,0x24,0x25,1,0x42,0,7);
    let first=store.transfer_character(&outbound).unwrap();
    assert!(first.applied);
    assert_eq!(first.authority_epoch,2);
    assert_eq!(first.snapshot_revision,1);
    assert_eq!(store.active_presence(id(0x7)).unwrap().unwrap().world_instance_id,id(0x23));
    let (rev,snapshot)=store.latest_state(id(0x7)).unwrap().unwrap();
    assert_eq!(rev,1);
    assert_eq!(snapshot.state,outbound.native_state.state);
    assert_eq!(snapshot.binary_artifact,Some(id(0x4)));
    assert_eq!(snapshot.revision,Revision(7));

    let replay=store.transfer_character(&outbound).unwrap();
    assert!(!replay.applied);
    assert_eq!(replay.authority_epoch,2);
    assert_eq!(replay.snapshot_revision,1);

    let inbound=command(0x43,"rust-pg-inbound",0x41,0x44,0x13,0x14,0x20,2,0x45,1,8);
    let second=store.transfer_character(&inbound).unwrap();
    assert!(second.applied);
    assert_eq!(second.authority_epoch,3);
    assert_eq!(second.snapshot_revision,2);
    assert_eq!(store.active_presence(id(0x7)).unwrap().unwrap().world_instance_id,id(0x13));
    assert_eq!(store.latest_state(id(0x7)).unwrap().unwrap().1.revision,Revision(8));
    // An old request always returns its ORIGINAL committed epoch/revision.
    assert_eq!(store.transfer_character(&outbound).unwrap(),replay);

    // A valid source epoch with an outdated native version must roll back
    // the entire attempted handoff. The character remains in the same world.
    let stale=command(0x46,"rust-pg-stale",0x44,0x47,0x23,0x24,0x25,3,0x48,0,9);
    assert!(store.transfer_character(&stale).is_err());
    let active=store.active_presence(id(0x7)).unwrap().unwrap();
    assert_eq!(active.world_instance_id,id(0x13));
    assert_eq!(active.authority_epoch,3);
    assert_eq!(store.latest_state(id(0x7)).unwrap().unwrap().0,2);
    assert_eq!(store.load_entity(id(0x7)).unwrap().unwrap().origin_module,Some(id(0x5)));
}
