//! Real child-process test: OASIS calls the original Cave Story player
//! and tile collision implementation, not a synthetic movement fixture.
//! The checkpoint used here intentionally covers the supported *movement*
//! fields only. It does NOT prove a lossless full-game scene checkpoint.
use std::collections::BTreeMap;

use oasis_adapter_cave_story::gameplay::{kind,OriginalPlayerAdapter,MODULE_ID,DEFINITION_ID};
use oasis_contracts::{
    AdapterRegistry,ClockStep,ContractError,DefinitionRef,EntityView,GameAdapter,
    Id,InputIntent,Revision,TypeRef,TypedValue,Value,
};
use oasis_runtime::{catalog::Catalog,Host};

fn generated_stage()->(Vec<u8>,Vec<u8>){
    const WIDTH:usize=12;
    const HEIGHT:usize=12;
    let mut tiles=vec![0u8;WIDTH*HEIGHT];
    for x in 0..WIDTH{tiles[7*WIDTH+x]=1;}
    let mut pxm=b"PXM".to_vec();
    pxm.push(0x10);
    pxm.extend_from_slice(&(WIDTH as u16).to_le_bytes());
    pxm.extend_from_slice(&(HEIGHT as u16).to_le_bytes());
    pxm.extend_from_slice(&tiles);
    let mut attributes=vec![0;256];
    attributes[1]=0x41;
    (pxm,attributes)
}
fn player()->EntityView{
    let (pxm,attributes)=generated_stage();
    EntityView{
        id:Id(u128::MAX-2),
        definition:DefinitionRef{id:DEFINITION_ID,version:1},
        revision:Revision(42),origin_module:Some(MODULE_ID),
        components:vec![
            TypedValue{type_ref:kind("player.stage-pxm"),data:Value::Map(BTreeMap::from([
                ("pxm".into(),Value::Bytes(pxm)),
                ("attributes".into(),Value::Bytes(attributes)),
            ]))},
            TypedValue{type_ref:kind("player.movement-state"),data:Value::Map(BTreeMap::from([
                ("x".into(),Value::Int(6*8192)),
                ("y".into(),Value::Int(2*8192)),
                ("vel_x".into(),Value::Int(0)),
                ("vel_y".into(),Value::Int(0)),
                ("life".into(),Value::UInt(3)),
                ("collision_flags".into(),Value::UInt(0)),
                ("native_tick".into(),Value::UInt(0)),
            ]))},
            TypedValue{type_ref:TypeRef{namespace:"original.unknown".into(),
                name:"opaque-native-bytecode".into(),version:9},
                data:Value::Bytes(vec![0,254,255,42])},
        ],
    }
}
fn adapter()->OriginalPlayerAdapter{
    OriginalPlayerAdapter{
        worker_path:env!("CARGO_BIN_EXE_oasis-worker").into(),
    }
}
fn tick(n:u64)->ClockStep{
    ClockStep{native_tick:n,simulation_time_nanos:u128::from(n)*20_000_000,
        delta_nanos:20_000_000}
}
fn controls(id:Id,right:bool)->InputIntent{
    InputIntent{
        controller_entity_id:id,
        intent:TypedValue{type_ref:kind("player.controls"),
            data:Value::Map(BTreeMap::from([
                ("right".into(),Value::Bool(right)),
            ]))},
    }
}
fn movement(s:&oasis_contracts::Snapshot)->&BTreeMap<String,Value>{
    let state=s.state.iter().find(|v|v.type_ref==kind("player.movement-state")).unwrap();
    let Value::Map(m)=&state.data else{panic!("malformed native state")};
    m
}
fn get_i32(m:&BTreeMap<String,Value>,key:&str)->i64{
    match m.get(key){Some(Value::Int(n))=>*n,_=>panic!("not native signed int")}
}

#[test]
fn actual_original_player_worker_uses_universal_catalog_and_host() {
    let original=player();
    let id=original.id;
    let adapter=adapter();
    let mut catalog=Catalog::new();
    catalog.install(&adapter).unwrap();
    assert_eq!(catalog.module(MODULE_ID).unwrap().engine_id,
        oasis_adapter_cave_story::ENGINE_ID);
    assert_eq!(catalog.capability_provider(&kind("player.controls")),Some(MODULE_ID));

    let mut host=Host::new();
    let context=Id(7001);
    host.register_module(context,adapter.start_context(MODULE_ID,context).unwrap()).unwrap();
    host.instantiate(context,original.clone(),None).unwrap();
    let before=host.snapshot(id).unwrap();
    assert_eq!(before.state,original.components);
    for n in 1..=8 {
        let output=host.step(context,tick(n),&[controls(id,true)]).unwrap();
        assert_eq!(output.state_changes.len(),1);
        assert_eq!(output.state_changes[0].revision,Revision(42+n));
    }
    let checkpoint=host.snapshot(id).unwrap();
    assert!(get_i32(movement(&checkpoint),"x")>6*8192);
    assert!(get_i32(movement(&checkpoint),"vel_x")>0);
    assert_eq!(movement(&checkpoint).get("native_tick"),Some(&Value::UInt(8)));
    assert_eq!(checkpoint.state[2],original.components[2]);

    // Native player and its source-game stage are reconstructed in a NEW
    // child OS process, carrying the original global entity ID unchanged.
    host.remove(id).unwrap();
    let mut recovery=Host::new();
    let second_context=Id(7002);
    recovery.register_module(second_context,
        adapter.start_context(MODULE_ID,second_context).unwrap()).unwrap();
    recovery.instantiate(second_context,original.clone(),Some(&checkpoint)).unwrap();
    assert_eq!(recovery.snapshot(id).unwrap().state,checkpoint.state);
    // A different world has its own scheduler clock starting from tick 1.
    // The source-native character must nevertheless continue from tick 9.
    for host_tick in 1..=8 {
        let output=recovery.step(second_context,tick(host_tick),&[controls(id,false)]).unwrap();
        assert_eq!(output.emitted_events[0].type_ref,kind("player.frame"));
    }
    let recovered=recovery.snapshot(id).unwrap();
    assert_eq!(recovered.entity_id,id);
    assert_eq!(recovered.revision,Revision(58));
    assert_eq!(recovered.state[2],original.components[2]);
    assert_eq!(movement(&recovered).get("native_tick"),Some(&Value::UInt(16)));
}
#[test]
fn invalid_native_controls_never_mutate_player_or_advance_tick(){
    let original=player();
    let adapter=adapter();
    let mut host=Host::new();
    let context=Id(7003);
    host.register_module(context,adapter.start_context(MODULE_ID,context).unwrap()).unwrap();
    host.instantiate(context,original.clone(),None).unwrap();
    let before=host.snapshot(original.id).unwrap();
    let foreign=InputIntent{
        controller_entity_id:Id(1),intent:TypedValue{
            type_ref:kind("player.controls"),
            data:Value::Map(BTreeMap::from([("right".into(),Value::Bool(true))])),
        },
    };
    assert!(host.step(context,tick(1),&[foreign]).is_err());
    assert_eq!(host.snapshot(original.id).unwrap().state,before.state);

    let bad=InputIntent{controller_entity_id:original.id,
        intent:TypedValue{type_ref:kind("player.controls"),
            data:Value::Map(BTreeMap::from([("right".into(),Value::UInt(1))]))},
    };
    assert!(host.step(context,tick(1),&[bad]).is_err());
    assert_eq!(host.snapshot(original.id).unwrap().revision,Revision(42));
    host.step(context,tick(1),&[controls(original.id,true)]).unwrap();
    assert_eq!(host.snapshot(original.id).unwrap().revision,Revision(43));
}
#[test]
fn native_player_checkpoint_rejects_foreign_identity_and_malformed_source_stage(){
    let mut entity=player();
    let adapter=adapter();
    let mut host=Host::new();
    let ctx=Id(7004);
    host.register_module(ctx,adapter.start_context(MODULE_ID,ctx).unwrap()).unwrap();
    let mut foreign=entity.clone();
    foreign.origin_module=Some(Id(999));
    assert!(host.instantiate(ctx,foreign,None).is_err());
    let Value::Map(m)=&mut entity.components[0].data else{panic!()};
    m.insert("pxm".into(),Value::Bytes(vec![1,2,3]));
    assert!(host.instantiate(ctx,entity,None).is_err());
}

#[test]
fn authoritative_world_runner_executes_original_player_in_another_world() {
    use std::sync::{Arc,Mutex};
    use std::time::{Duration,SystemTime,Instant};
    use oasis_runtime::scheduler::{WorldRunner,TickAuthorizer};
    use oasis_runtime::universe::{
        Universe,Principal,PersistedPlacement,SessionAuthority,SimulationLease,
    };
    use oasis_contracts::ContractResult;

    struct TestAuthority;
    impl SessionAuthority for TestAuthority {
        fn authenticate(&self,credential:&str)->ContractResult<Principal>{
            if credential!="trusted-session"{
                return Err(ContractError::InvalidData("invalid session".into()));
            }
            Ok(Principal{
                user_id:Id(7),player_id:Id(8),character_id:Id(u128::MAX-2),
                expires_at:SystemTime::now()+Duration::from_secs(60),
            })
        }
    }
    struct TestLease;
    impl TickAuthorizer for TestLease {
        fn authorize(&self,_world:Id,lease:SimulationLease)->ContractResult<()>{
            if lease.epoch!=1 {
                return Err(ContractError::StaleAuthority);
            }
            Ok(())
        }
    }

    let source_world=Id(0x8000);
    let destination_world=Id(0x9000);
    let first_context=Id(0x8001);
    let second_context=Id(0x9001);
    let adapter=adapter();
    let entity=player();
    let mut universe=Universe::new();
    universe.add_world(source_world).unwrap();
    universe.add_world(destination_world).unwrap();
    universe.add_module(source_world,first_context,
        adapter.start_context(MODULE_ID,first_context).unwrap()).unwrap();
    universe.add_module(destination_world,second_context,
        adapter.start_context(MODULE_ID,second_context).unwrap()).unwrap();
    universe.recover_entity(entity.clone(),None,Some(PersistedPlacement{
        world_instance_id:source_world,
        authority_context_id:first_context,
        authority_epoch:1,
    })).unwrap();

    let ticket=universe.connect(&TestAuthority,"trusted-session").unwrap();
    let source_state=universe.observe(ticket).unwrap();
    assert_eq!(source_state[0].state,entity.components);
    // The source-native module is independently instantiated in the
    // destination world, with no game-pair or per-world compatibility code.
    universe.enter_world(ticket,destination_world).unwrap();
    assert_eq!(universe.presence(entity.id),Some(destination_world));
    assert_eq!(universe.observe(ticket).unwrap()[0].entity_id,entity.id);
    universe.input(ticket,TypedValue{
        type_ref:kind("player.controls"),
        data:Value::Map(BTreeMap::from([
            ("right".into(),Value::Bool(true))
        ])),
    }).unwrap();

    let shared=Arc::new(Mutex::new(universe));
    let runner=WorldRunner::start(
        Arc::clone(&shared),destination_world,
        Duration::from_millis(10),Arc::new(TestLease),
    ).unwrap();
    let deadline=Instant::now()+Duration::from_secs(2);
    loop {
        let progressed={
            let mut guard=shared.lock().unwrap();
            guard.observe(ticket).unwrap()[0].revision.0>entity.revision.0
        };
        if progressed || Instant::now()>=deadline {
            assert!(progressed,"source native player was not ticked by authoritative world runner");
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(runner.last_error().is_none(),"original native worker lost authority during tick");
    drop(runner);
    let mut guard=shared.lock().unwrap();
    let updated=guard.observe(ticket).unwrap();
    assert_eq!(updated[0].entity_id,entity.id);
    assert!(updated[0].revision.0>entity.revision.0);
    assert_eq!(updated[0].state[2],entity.components[2]);
}

#[test]
fn empty_world_does_not_require_an_original_player_to_tick(){
    let adapter=adapter();
    let mut host=Host::new();
    let ctx=Id(7010);
    host.register_module(ctx,adapter.start_context(MODULE_ID,ctx).unwrap()).unwrap();
    let empty=host.step(ctx,tick(1),&[]).unwrap();
    assert!(empty.state_changes.is_empty());
    assert!(empty.emitted_events.is_empty());

    // A character can then arrive after the destination world's clock has
    // been running independently of this originating game module.
    let entity=player();
    host.instantiate(ctx,entity.clone(),None).unwrap();
    let outcome=host.step(ctx,tick(2),&[controls(entity.id,true)]).unwrap();
    assert_eq!(outcome.state_changes.len(),1);
    assert_eq!(outcome.state_changes[0].entity_id,entity.id);
}

#[test]
fn original_projectile_fires_inside_native_game_worker_and_consumes_source_ammo(){
    let mut entity=player();
    entity.components.push(TypedValue{
        type_ref:kind("player.native-weapon"),
        data:Value::Map(BTreeMap::from([
            ("weapon_type".into(),Value::UInt(2)), // source Polar Star
            ("level".into(),Value::UInt(1)),
            ("experience".into(),Value::UInt(0)),
            ("ammo".into(),Value::UInt(3)),
            ("max_ammo".into(),Value::UInt(3)),
        ])),
    });
    let id=entity.id;
    let adapter=adapter();
    let mut host=Host::new();
    let ctx=Id(7020);
    host.register_module(ctx,adapter.start_context(MODULE_ID,ctx).unwrap()).unwrap();
    host.instantiate(ctx,entity.clone(),None).unwrap();
    let input=InputIntent{
        controller_entity_id:id,
        intent:TypedValue{
            type_ref:kind("player.controls"),
            data:Value::Map(BTreeMap::from([
                ("shoot".into(),Value::Bool(true)),
            ])),
        },
    };
    let output=host.step(ctx,tick(1),&[input]).unwrap();
    let count=output.emitted_events.iter().find(|e|e.type_ref==kind("player.native-projectiles"))
        .expect("upstream source projectile event");
    assert!(matches!(count.data,Value::UInt(n) if n>0),
        "real upstream engine must spawn its own native projectile");

    let native=host.snapshot(id).unwrap();
    let weapon=native.state.iter().find(|p|p.type_ref==kind("player.native-weapon")).unwrap();
    let Value::Map(fields)=&weapon.data else{panic!("wrong source weapon checkpoint")};
    assert_eq!(fields.get("ammo"),Some(&Value::UInt(2)));
    assert_eq!(fields.get("max_ammo"),Some(&Value::UInt(3)));
    assert_eq!(fields.get("weapon_type"),Some(&Value::UInt(2)));

    // The source-defined inventory state, not a converted partner item,
    // remains available when another engine process hosts the same entity.
    host.remove(id).unwrap();
    let mut restarted=Host::new();
    let other=Id(7021);
    restarted.register_module(other,adapter.start_context(MODULE_ID,other).unwrap()).unwrap();
    restarted.instantiate(other,entity.clone(),Some(&native)).unwrap();
    let saved=restarted.snapshot(id).unwrap();
    assert_eq!(saved.state,native.state);
}
