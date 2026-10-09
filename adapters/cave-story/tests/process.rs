//! Exercise the pinned upstream Cave Story source code in REAL separate
//! game-worker processes, not the synthetic native IPC fixture.
use std::collections::BTreeMap;
use std::process::Command;

use oasis_contracts::{
    ClockStep,ContractError,ContractResult,DefinitionRef,EntityView,FrameMap,
    GeometryRequest,GeometryResult,Id,InteractionRequest,InputIntent,ModuleDescriptor,
    NativeModule,Revision,Snapshot,SpatialHit,SpatialQuery,TypeRef,TypedValue,Value,WorldPort,
};
use oasis_native_process::ProcessModule;
use oasis_adapter_cave_story::{self as game,stage,weapon};

struct World;
impl WorldPort for World {
    fn geometry(&mut self,_:GeometryRequest)->ContractResult<GeometryResult>{
        Err(ContractError::InvalidData("unexpected geometry query".into()))
    }
    fn query(&mut self,_:SpatialQuery)->ContractResult<Vec<SpatialHit>>{
        Err(ContractError::InvalidData("unexpected spatial query".into()))
    }
    fn frame_map(&self,_:Id,_:Id)->ContractResult<FrameMap>{
        Err(ContractError::InvalidData("unexpected frame query".into()))
    }
    fn entity_view(&self,_:Id)->ContractResult<EntityView>{
        Err(ContractError::InvalidData("unexpected entity query".into()))
    }
    fn submit_interaction(&mut self,_:InteractionRequest)->ContractResult<()>{
        Err(ContractError::InvalidData("unexpected external effect".into()))
    }
}
fn field(map:BTreeMap<String,Value>)->Value{Value::Map(map)}
fn typ(name:&str)->TypeRef{
    TypeRef{namespace:"cave-story".into(),name:name.into(),version:1}
}
fn entity(id:Id,module:Id,definition:Id,component:TypedValue)->EntityView{
    EntityView{
        id,definition:DefinitionRef{id:definition,version:1},
        revision:Revision(5),components:vec![component],origin_module:Some(module),
    }
}
fn intent(id:Id,name:&str,data:Value)->InputIntent{
    InputIntent{
        controller_entity_id:id,intent:TypedValue{type_ref:typ(name),data},
    }
}
fn clock()->ClockStep{
    ClockStep{native_tick:1,simulation_time_nanos:20_000_000,delta_nanos:20_000_000}
}
fn native(kind:&str,module:Id,context:Id)->ProcessModule{
    let mut command=Command::new(env!("CARGO_BIN_EXE_oasis-worker"));
    command.arg(kind).arg(context.0.to_string());
    ProcessModule::spawn(&mut command,module,context).unwrap()
}
fn result_map(out:&TypedValue)->&BTreeMap<String,Value>{
    let Value::Map(map)=&out.data else{panic!("source-native result is not a map")};
    map
}
#[test]
fn cave_story_actual_original_hitbox_runs_in_an_isolated_game_process(){
    let id=Id(0x111);
    let module=game::MODULE_ID;
    let mut game=native("hitbox",module,Id(700));
    let description:ModuleDescriptor=game.descriptor();
    assert_eq!(description.module_id,module);
    let entity=entity(id,module,game::DEFINITION_ID,TypedValue{
        type_ref:typ("hitbox"),
        data:field(BTreeMap::from([
            ("x".into(),Value::Int(1000)),
            ("y".into(),Value::Int(2000)),
            ("left".into(),Value::UInt(20)),
            ("right".into(),Value::UInt(30)),
            ("top".into(),Value::UInt(50)),
            ("bottom".into(),Value::UInt(60)),
        ])),
    });
    let handle=game.instantiate(&entity,None).unwrap();
    assert_eq!(handle.context_id,Id(700));
    let out=game.step(clock(),&[intent(id,"hit-test-point",field(BTreeMap::from([
        ("x".into(),Value::Int(980)),("y".into(),Value::Int(2000)),
    ])))],&mut World).unwrap();
    assert_eq!(result_map(&out.emitted_events[0]).get("inside"),
        Some(&Value::Bool(false))); // Original exclusive left boundary.
    assert_eq!(game.snapshot(handle).unwrap().state,entity.components);
}
#[test]
fn cave_story_original_weapon_ammunition_survives_native_process_snapshots(){
    let id=Id(u128::MAX);
    let mut process=native("weapon",weapon::MODULE_ID,Id(701));
    let entity=entity(id,weapon::MODULE_ID,weapon::DEFINITION_ID,TypedValue{
        type_ref:typ("weapon-native"),
        data:field(BTreeMap::from([
            ("weapon_type".into(),Value::UInt(2)),
            ("level".into(),Value::UInt(2)),
            ("experience".into(),Value::UInt(9)),
            ("ammo".into(),Value::UInt(5)),
            ("max_ammo".into(),Value::UInt(8)),
            ("original_metadata".into(),Value::Bytes(vec![0,255,1])),
        ])),
    });
    let handle=process.instantiate(&entity,None).unwrap();
    let out=process.step(clock(),&[intent(id,"weapon.consume-ammo",
        Value::UInt(3))],&mut World).unwrap();
    let map=result_map(&out.emitted_events[0]);
    assert_eq!(map.get("accepted"),Some(&Value::Bool(true)));
    assert_eq!(map.get("ammo"),Some(&Value::UInt(2)));
    let checkpoint:Snapshot=process.snapshot(handle).unwrap();
    let Value::Map(state)=&checkpoint.state[0].data else{panic!("bad native state")};
    assert_eq!(state.get("ammo"),Some(&Value::UInt(2)));
    assert_eq!(state.get("original_metadata"),Some(&Value::Bytes(vec![0,255,1])));
    process.remove(handle).unwrap();

    // A newly created original source engine process continues this object
    // with the same globally unique ID and original native gameplay data.
    let mut restarted=native("weapon",weapon::MODULE_ID,Id(702));
    let handle=restarted.instantiate(&entity,Some(&checkpoint)).unwrap();
    assert_eq!(restarted.snapshot(handle).unwrap().state,checkpoint.state);
    let out=restarted.step(clock(),&[intent(id,"weapon.refill-ammo",
        Value::UInt(9))],&mut World).unwrap();
    assert_eq!(result_map(&out.emitted_events[0]).get("ammo"),
        Some(&Value::UInt(8)));
}
#[test]
fn cave_story_upstream_stage_pxm_reader_runs_in_an_isolated_game_process(){
    let id=Id(0x555);
    let mut native=native("stage",stage::MODULE_ID,Id(703));
    let mut pxm=b"PXM".to_vec();
    pxm.push(0x10);
    pxm.extend_from_slice(&2u16.to_le_bytes());
    pxm.extend_from_slice(&1u16.to_le_bytes());
    pxm.extend_from_slice(&[1,2]);
    let mut attr=vec![0u8;256];
    attr[1]=0x41;attr[2]=0x62;
    let entity=entity(id,stage::MODULE_ID,stage::DEFINITION_ID,TypedValue{
        type_ref:typ("stage.pxm"),
        data:field(BTreeMap::from([
            ("pxm".into(),Value::Bytes(pxm)),
            ("attributes".into(),Value::Bytes(attr)),
        ])),
    });
    let handle=native.instantiate(&entity,None).unwrap();
    let out=native.step(clock(),&[intent(id,"stage.tile-attribute",
        field(BTreeMap::from([
            ("x".into(),Value::UInt(1)),
            ("y".into(),Value::UInt(0)),
        ])))],&mut World).unwrap();
    assert_eq!(result_map(&out.emitted_events[0]).get("attribute"),
        Some(&Value::UInt(0x62)));
    assert_eq!(native.snapshot(handle).unwrap().state,entity.components);
}
