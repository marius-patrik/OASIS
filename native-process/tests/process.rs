use std::cell::Cell;
use std::collections::BTreeMap;
use std::process::Command;

use oasis_contracts::{
    ClockStep,ContractError,ContractResult,DefinitionRef,EntityView,FrameMap,
    GeometryRequest,GeometryResult,Id,InputIntent,InteractionRequest,
    ModuleDescriptor,NativeModule,Revision,SpatialHit,SpatialQuery,
    TypeRef,TypedValue,Value,WorldPort,
};
use oasis_native_process::ProcessModule;

const MODULE:Id=Id(0x123);
const CONTEXT:Id=Id(0x789);

struct World {
    actor:EntityView,
    calls:Cell<usize>,
}
impl WorldPort for World {
    fn geometry(&mut self,request:GeometryRequest)->ContractResult<GeometryResult>{
        assert_eq!(request.frame_id,Id(11));
        self.calls.set(self.calls.get()+1);
        Ok(GeometryResult{shapes:vec![],revision:Revision(2)})
    }
    fn query(&mut self,request:SpatialQuery)->ContractResult<Vec<SpatialHit>>{
        assert_eq!(request.frame_id,Id(11));
        self.calls.set(self.calls.get()+1);
        Ok(vec![])
    }
    fn frame_map(&self,source:Id,destination:Id)->ContractResult<FrameMap>{
        assert_eq!((source,destination),(Id(11),Id(12)));
        self.calls.set(self.calls.get()+1);
        Ok(FrameMap{
            source,destination,column_major_4x4:[1.0;16],
            source_units_per_destination_unit:1.0,
        })
    }
    fn entity_view(&self,entity:Id)->ContractResult<EntityView>{
        assert_eq!(entity,self.actor.id);
        self.calls.set(self.calls.get()+1);
        Ok(self.actor.clone())
    }
    fn submit_interaction(&mut self,request:InteractionRequest)->ContractResult<()>{
        assert_eq!(request.id,Id(44));
        let Value::Map(data)=&request.operation.data else{panic!("native data truncated")};
        assert_eq!(data.get("bytecode"),Some(&Value::Bytes(vec![0,1,255])));
        assert_eq!(data.get("identity"),Some(&Value::Ref(Id(u128::MAX))));
        self.calls.set(self.calls.get()+1);
        Ok(())
    }
}
fn actor()->EntityView{
    EntityView{
        id:Id(u128::MAX),
        definition:DefinitionRef{id:Id(0x22),version:1},
        revision:Revision(23),
        origin_module:Some(MODULE),
        components:vec![TypedValue{
            type_ref:TypeRef{
                namespace:"source".into(),name:"opaque".into(),version:17,
            },
            data:Value::Map(BTreeMap::from([
                ("binary".into(),Value::Bytes(vec![0,1,254,255])),
                ("unsigned".into(),Value::UInt(u64::MAX)),
                ("reference".into(),Value::Ref(Id(u128::MAX))),
            ])),
        }],
    }
}
fn native_command()->Command{
    Command::new(env!("CARGO_BIN_EXE_fixture-worker"))
}

#[test]
fn real_process_isolation_native_state_and_all_world_callbacks_roundtrip(){
    let mut process=ProcessModule::spawn(&mut native_command(),MODULE,CONTEXT).unwrap();
    let desc:ModuleDescriptor=process.descriptor();
    assert_eq!(desc.module_id,MODULE);
    let entity=actor();
    let handle=process.instantiate(&entity,None).unwrap();
    assert_eq!(handle.context_id,CONTEXT);

    let before=process.snapshot(handle).unwrap();
    assert_eq!(before.state,entity.components);
    let mut world=World{actor:entity.clone(),calls:Cell::new(0)};
    let output=process.step(ClockStep{
        native_tick:1,simulation_time_nanos:u128::MAX,
        delta_nanos:28_571_428,
    },&[InputIntent{
        controller_entity_id:entity.id,
        intent:TypedValue{
            type_ref:TypeRef{namespace:"fixture".into(),
                name:"native-action".into(),version:1},
            data:Value::Bytes(vec![255,0,1]),
        },
    }],&mut world).unwrap();
    assert_eq!(world.calls.get(),5);
    assert_eq!(output.state_changes.len(),1);
    assert_eq!(output.state_changes[0].state,entity.components);
    assert_eq!(process.snapshot(handle).unwrap().state,entity.components);

    let mut saved=before.clone();
    saved.revision=Revision(70);
    saved.binary_artifact=Some(Id(u128::MAX));
    process.restore(handle,&saved).unwrap();
    let after=process.snapshot(handle).unwrap();
    assert_eq!(after.revision,Revision(70));
    assert_eq!(after.binary_artifact,Some(Id(u128::MAX)));
    assert_eq!(after.state,before.state);

    let mut foreign=after.clone();
    foreign.entity_id=Id(1);
    assert!(matches!(process.restore(handle,&foreign),Err(ContractError::InvalidData(_))));
    assert_eq!(process.snapshot(handle).unwrap().revision,Revision(70));
    process.remove(handle).unwrap();
    assert!(process.snapshot(handle).is_err());
}

#[test]
fn handshake_rejects_unexpected_native_game_identity(){
    let attempted=ProcessModule::spawn(&mut native_command(),Id(999),CONTEXT);
    assert!(attempted.is_err());
}

#[test]
fn source_process_never_authorizes_unloaded_or_foreign_controllers(){
    let mut process=ProcessModule::spawn(&mut native_command(),MODULE,CONTEXT).unwrap();
    let entity=actor();
    let mut world=World{actor:entity.clone(),calls:Cell::new(0)};
    let foreign=InputIntent{
        controller_entity_id:Id(0),
        intent:TypedValue{
            type_ref:TypeRef{namespace:"fixture".into(),name:"native-action".into(),version:1},
            data:Value::Null,
        },
    };
    process.instantiate(&entity,None).unwrap();
    let result=process.step(ClockStep{
        native_tick:1,simulation_time_nanos:1,delta_nanos:1,
    },&[foreign],&mut world);
    assert!(matches!(result,Err(ContractError::InvalidData(_))));
    assert_eq!(world.calls.get(),0);
    let result=process.step(ClockStep{
        native_tick:1,simulation_time_nanos:1,delta_nanos:1,
    },&[],&mut world);
    assert!(result.is_ok());
    assert_eq!(world.calls.get(),5);
}

#[test]
fn worker_wire_rejects_nan_and_preserves_maximal_global_identifiers(){
    assert!(serde_json::to_vec(&Value::Float(f64::NAN)).is_err());
    assert!(serde_json::to_vec(&Value::Float(f64::INFINITY)).is_err());
    let encoded=serde_json::to_vec(&Value::Ref(Id(u128::MAX))).unwrap();
    let decoded:Value=serde_json::from_slice(&encoded).unwrap();
    assert_eq!(decoded,Value::Ref(Id(u128::MAX)));
    let invalid=serde_json::from_str::<Value>(
        r#"{"Float":"NaN"}"#,
    );
    assert!(invalid.is_err());
}
