//! Invoke actual original DOOM numeric and geometry functions in their own
//! GPL-scoped worker OS process. No engine gameplay globals are shared.
use std::collections::BTreeMap;
use std::process::Command;
use oasis_adapter_doom::{MODULE_ID,DEFINITION_ID};
use oasis_contracts::{
    ClockStep,ContractError,ContractResult,DefinitionRef,EntityView,FrameMap,
    GeometryRequest,GeometryResult,Id,InputIntent,InteractionRequest,
    NativeModule,Revision,SpatialHit,SpatialQuery,TypeRef,TypedValue,Value,WorldPort,
};
use oasis_native_process::ProcessModule;
struct World;
impl WorldPort for World{
    fn geometry(&mut self,_:GeometryRequest)->ContractResult<GeometryResult>{
        Err(ContractError::InvalidData("not expected".into()))
    }
    fn query(&mut self,_:SpatialQuery)->ContractResult<Vec<SpatialHit>>{
        Err(ContractError::InvalidData("not expected".into()))
    }
    fn frame_map(&self,_:Id,_:Id)->ContractResult<FrameMap>{
        Err(ContractError::InvalidData("not expected".into()))
    }
    fn entity_view(&self,_:Id)->ContractResult<EntityView>{
        Err(ContractError::InvalidData("not expected".into()))
    }
    fn submit_interaction(&mut self,_:InteractionRequest)->ContractResult<()>{
        Err(ContractError::InvalidData("not expected".into()))
    }
}
#[test]
fn original_doom_fixed_point_physics_runs_inside_its_own_process(){
    let context=Id(703);
    let mut cmd=Command::new(env!("CARGO_BIN_EXE_oasis-worker"));
    cmd.arg(context.0.to_string());
    let mut module=ProcessModule::spawn(&mut cmd,MODULE_ID,context).unwrap();
    let e=EntityView{
        id:Id(u128::MAX),
        definition:DefinitionRef{id:DEFINITION_ID,version:1},
        revision:Revision(99),
        origin_module:Some(MODULE_ID),
        components:vec![TypedValue{
            type_ref:TypeRef{namespace:"doom".into(),name:"native-opaque".into(),version:1},
            data:Value::Bytes(vec![255,0,12]),
        }],
    };
    let handle=module.instantiate(&e,None).unwrap();
    let output=module.step(ClockStep{
        native_tick:1,simulation_time_nanos:28_571_428,
        delta_nanos:28_571_428,
    },&[InputIntent{
        controller_entity_id:e.id,
        intent:TypedValue{
            type_ref:TypeRef{namespace:"doom".into(),name:"fixed-mul".into(),version:1},
            data:Value::Map(BTreeMap::from([
                ("a".into(),Value::Int(3<<15)),
                ("b".into(),Value::Int(2<<16)),
            ])),
        },
    }],&mut World).unwrap();
    let Value::Map(output)=&output.emitted_events[0].data else{
        panic!("DOOM result was not typed");
    };
    assert_eq!(output.get("result"),Some(&Value::Int(3<<16)));
    assert_eq!(module.snapshot(handle).unwrap().state,e.components);
    module.remove(handle).unwrap();
}
