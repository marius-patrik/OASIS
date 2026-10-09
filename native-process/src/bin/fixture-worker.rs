//! Executable contract fixture; one process per native execution context.
//! This fixture deliberately depends on neither DOOM nor Cave Story.
use oasis_contracts::{
    AuthorityStamp,ClockStep,ContractError,ContractResult,
    EntityView,FrameMap,GeometryRequest,Id,InputIntent,InteractionRequest,
    InteractionTarget,ModuleDescriptor,NativeHandle,Snapshot,
    SpatialQuery,StepOutput,TypeRef,TypedValue,Value,Vec3,WorldPort,
};
use oasis_native_process::{serve_worker,LocalNativeModule};

const MODULE:Id=Id(0x123);
const CONTEXT:Id=Id(0x789);

struct Fixture { live:Option<Snapshot>, tick:u64 }
fn bad(reason:&str)->ContractError{
    ContractError::InvalidData(reason.into())
}
fn kind(name:&str)->TypeRef {
    TypeRef{namespace:"fixture".into(),name:name.into(),version:1}
}
impl LocalNativeModule for Fixture {
    fn descriptor(&self)->ModuleDescriptor{
        ModuleDescriptor{
            engine_id:Id(0x1),module_id:MODULE,
            contract_major:0,contract_minor:1,
            exported_interfaces:vec![kind("native-action")],
            required_interfaces:vec![],
        }
    }
    fn instantiate(&mut self,entity:&EntityView,state:Option<&Snapshot>)
        ->ContractResult<NativeHandle>{
        if entity.origin_module!=Some(MODULE)||self.live.is_some(){
            return Err(bad("wrong origin or already loaded"));
        }
        let snapshot=state.cloned().unwrap_or(Snapshot{
            entity_id:entity.id,revision:entity.revision,
            state:entity.components.clone(),binary_artifact:None,
        });
        if snapshot.entity_id!=entity.id{return Err(bad("foreign snapshot"));}
        self.live=Some(snapshot);
        Ok(NativeHandle{context_id:CONTEXT,native_slot:1})
    }
    fn step(&mut self,clock:ClockStep,inputs:&[InputIntent],world:&mut dyn WorldPort)
        ->ContractResult<StepOutput>{
        let Some(snapshot)=self.live.as_ref() else{return Err(bad("no native entity"));};
        if clock.native_tick<=self.tick{return Err(bad("stale tick"));}
        let id=snapshot.entity_id;
        if inputs.iter().any(|i|i.controller_entity_id!=id){
            return Err(bad("foreign controller"));
        }
        let _geometry=world.geometry(GeometryRequest{
            frame_id:Id(11),center:Vec3{x:1.0,y:2.0,z:3.0},
            radius:15.0,filter:None,
        })?;
        let _hits=world.query(SpatialQuery{
            frame_id:Id(11),
            query:TypedValue{type_ref:kind("nearby"),data:Value::Ref(id)},
        })?;
        let _map:FrameMap=world.frame_map(Id(11),Id(12))?;
        let _entity:EntityView=world.entity_view(id)?;
        world.submit_interaction(InteractionRequest{
            id:Id(44),source_entity_id:id,
            target:InteractionTarget::Entity(id),
            operation:TypedValue{
                type_ref:kind("action"),
                data:Value::Map(std::collections::BTreeMap::from([
                    ("bytecode".into(),Value::Bytes(vec![0,1,255])),
                    ("identity".into(),Value::Ref(Id(u128::MAX))),
                ])),
            },
            authority:AuthorityStamp{
                resource_key:"fixture".into(),context_id:CONTEXT,epoch:1,
            },
            source_tick:clock.native_tick,
        })?;
        self.tick=clock.native_tick;
        Ok(StepOutput{
            state_changes:vec![snapshot.clone()],
            interactions:vec![],
            emitted_events:vec![TypedValue{
                type_ref:kind("native-continued"),
                data:Value::UInt(clock.native_tick),
            }],
        })
    }
    fn snapshot(&self,handle:NativeHandle)->ContractResult<Snapshot>{
        if handle != (NativeHandle{context_id:CONTEXT,native_slot:1}) {
            return Err(ContractError::StaleAuthority);
        }
        self.live.clone().ok_or_else(||bad("no entity"))
    }
    fn restore(&mut self,handle:NativeHandle,snapshot:&Snapshot)->ContractResult<()>{
        let existing=self.snapshot(handle)?;
        if existing.entity_id!=snapshot.entity_id{return Err(bad("foreign checkpoint"));}
        self.live=Some(snapshot.clone());
        Ok(())
    }
    fn remove(&mut self,handle:NativeHandle)->ContractResult<()>{
        self.snapshot(handle)?;
        self.live=None;
        Ok(())
    }
}
fn main(){
    serve_worker(Fixture{live:None,tick:0}).expect("native worker protocol failed");
}
