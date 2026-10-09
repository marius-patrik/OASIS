//! Source-native Cave Story stage maps, loaded by the pinned upstream PXM
//! reader and queried through its real tile-attribute lookup algorithm.
//!
//! World stage data belongs to the origin game's engine. This module does
//! not substitute a universal 2D collision algorithm or interpret native
//! tile attributes as foreign-world material/physics semantics.

use std::collections::BTreeMap;
use std::io::Cursor;

use doukutsu_rs::game::map::Map;
use oasis_contracts::{
    AdapterRegistry, ClockStep, ContractError, ContractResult, DefinitionRef,
    EntityView, GameAdapter, Id, InputIntent, ModuleDescriptor, NativeHandle,
    NativeModule, Snapshot, StepOutput, TypeRef, TypedValue, Value, WorldPort,
};

use crate::ENGINE_ID;

pub const MODULE_ID: Id = Id(0xc451_0022);
pub const DEFINITION_ID: Id = Id(0xc451_0023);
const STAGE_DATA: &str = "stage.pxm";
const TILE_QUERY: &str = "stage.tile-attribute";
const TILE_RESULT: &str = "stage.tile-result";

fn kind(name: &str) -> TypeRef {
    TypeRef { namespace: "cave-story".into(), name: name.into(), version: 1 }
}
fn invalid(reason: &str) -> ContractError {
    ContractError::InvalidData(reason.into())
}
fn map_value(value: &Value) -> ContractResult<&BTreeMap<String, Value>> {
    match value {
        Value::Map(map) => Ok(map),
        _ => Err(invalid("Cave Story native stage expects source-typed map data")),
    }
}
fn bytes<'a>(map: &'a BTreeMap<String, Value>, key: &str) -> ContractResult<&'a [u8]> {
    match map.get(key) {
        Some(Value::Bytes(blob)) => Ok(blob),
        _ => Err(invalid("native stage PXM and tile-attribute bytes are mandatory")),
    }
}
fn coordinate(map: &BTreeMap<String, Value>, key: &str) -> ContractResult<usize> {
    match map.get(key) {
        Some(Value::UInt(u)) => usize::try_from(*u)
            .map_err(|_| invalid("native tile coordinate exceeds platform range")),
        _ => Err(invalid("native tile coordinate must be an unsigned integer")),
    }
}

struct StageRecord {
    entity_id: Id,
    snapshot: Snapshot,
    map: Map,
}
impl StageRecord {
    fn load_map(state: &[TypedValue]) -> ContractResult<Map> {
        let component = state.iter().find(|v|v.type_ref == kind(STAGE_DATA))
            .ok_or_else(|| invalid("native stage snapshot lacks PXM component"))?;
        let data=map_value(&component.data)?;
        let pxm=bytes(data,"pxm")?;
        let attributes=bytes(data,"attributes")?;

        if pxm.len()<8 || &pxm[0..3]!=b"PXM" || pxm[3]!=0x10 {
            return Err(invalid("native PXM header or version is invalid"));
        }
        let width=usize::from(u16::from_le_bytes([pxm[4],pxm[5]]));
        let height=usize::from(u16::from_le_bytes([pxm[6],pxm[7]]));
        let count=width.checked_mul(height)
            .ok_or_else(|| invalid("native map dimensions overflow"))?;
        // Pinned upstream Map::load_pxm currently multiplies u16 dimensions
        // before allocating; reject overflow rather than panicking in
        // debug builds. This limit is specific to the upstream reader.
        if width==0 || height==0 || count>usize::from(u16::MAX)
            || pxm.len()!=count+8 || attributes.len()!=256
        {
            return Err(invalid("native PXM dimensions or attribute table invalid"));
        }
        Map::load_pxm(Cursor::new(pxm),Cursor::new(attributes))
            .map_err(|error|ContractError::Internal(
                format!("upstream Cave Story PXM load failed: {error:?}")
            ))
    }
}

pub struct CaveStoryStageAdapter;
impl GameAdapter for CaveStoryStageAdapter {
    fn register(&self,registry:&mut dyn AdapterRegistry)->ContractResult<()>{
        registry.register_module(ModuleDescriptor{
            engine_id:ENGINE_ID,module_id:MODULE_ID,
            contract_major:0,contract_minor:1,
            exported_interfaces:vec![kind(TILE_QUERY)],
            required_interfaces:vec![],
        })?;
        registry.register_definition(DefinitionRef{id:DEFINITION_ID,version:1})?;
        registry.register_capability(kind(TILE_QUERY),MODULE_ID)
    }
    fn start_context(&self,module:Id,context:Id)->ContractResult<Box<dyn NativeModule>>{
        if module!=MODULE_ID{return Err(ContractError::NotFound(module));}
        Ok(Box::new(CaveStoryStageModule{
            context_id:context,next_slot:0,last_tick:0,records:BTreeMap::new(),
        }))
    }
}

pub struct CaveStoryStageModule {
    context_id: Id,
    next_slot: u64,
    last_tick: u64,
    records: BTreeMap<u64,StageRecord>,
}
impl CaveStoryStageModule {
    fn record(&self,handle:NativeHandle)->ContractResult<&StageRecord>{
        if handle.context_id!=self.context_id{return Err(ContractError::StaleAuthority);}
        self.records.get(&handle.native_slot)
            .ok_or(ContractError::NotFound(self.context_id))
    }
}
impl NativeModule for CaveStoryStageModule {
    fn descriptor(&self)->ModuleDescriptor{
        ModuleDescriptor{
            engine_id:ENGINE_ID,module_id:MODULE_ID,
            contract_major:0,contract_minor:1,
            exported_interfaces:vec![kind(TILE_QUERY)],
            required_interfaces:vec![],
        }
    }
    fn instantiate(&mut self,entity:&EntityView,saved:Option<&Snapshot>)
        ->ContractResult<NativeHandle>{
        if entity.origin_module!=Some(MODULE_ID){
            return Err(invalid("stage must be owned by the original Cave Story module"));
        }
        let snapshot=saved.cloned().unwrap_or_else(||Snapshot{
            entity_id:entity.id,revision:entity.revision,
            state:entity.components.clone(),binary_artifact:None,
        });
        if snapshot.entity_id!=entity.id {
            return Err(invalid("native stage snapshot identity mismatch"));
        }
        if self.records.values().any(|record|record.entity_id==entity.id){
            return Err(invalid("native stage is already loaded"));
        }
        let map=StageRecord::load_map(&snapshot.state)?;
        let slot=self.next_slot.checked_add(1)
            .ok_or_else(||invalid("stage native handle exhausted"))?;
        self.next_slot=slot;
        self.records.insert(slot,StageRecord{entity_id:entity.id,snapshot,map});
        Ok(NativeHandle{context_id:self.context_id,native_slot:slot})
    }
    fn step(&mut self,clock:ClockStep,inputs:&[InputIntent],_world:&mut dyn WorldPort)
        ->ContractResult<StepOutput>{
        if clock.native_tick<=self.last_tick{
            return Err(invalid("Cave Story native clock must advance"));
        }
        let mut results=Vec::with_capacity(inputs.len());
        for input in inputs {
            if input.intent.type_ref!=kind(TILE_QUERY){
                return Err(ContractError::Unsupported{
                    interface:input.intent.type_ref.clone(),
                    reason:"Cave Story stage supports its original tile attributes".into(),
                });
            }
            let fields=map_value(&input.intent.data)?;
            let x=coordinate(fields,"x")?;
            let y=coordinate(fields,"y")?;
            let instance=self.records.values()
                .find(|instance|instance.entity_id==input.controller_entity_id)
                .ok_or(ContractError::NotFound(input.controller_entity_id))?;
            let inside=x<usize::from(instance.map.width)
                && y<usize::from(instance.map.height);
            // The actual game defines native tile-to-attribute lookup.
            let attribute=instance.map.get_attribute(x,y);
            let tile=if inside{
                instance.map.tiles[usize::from(instance.map.width)*y+x]
            }else{0};
            results.push(TypedValue{
                type_ref:kind(TILE_RESULT),
                data:Value::Map(BTreeMap::from([
                    ("stage".into(),Value::Ref(instance.entity_id)),
                    ("x".into(),Value::UInt(x as u64)),
                    ("y".into(),Value::UInt(y as u64)),
                    ("in_bounds".into(),Value::Bool(inside)),
                    ("tile".into(),Value::UInt(u64::from(tile))),
                    ("attribute".into(),Value::UInt(u64::from(attribute))),
                ])),
            });
        }
        self.last_tick=clock.native_tick;
        Ok(StepOutput{
            state_changes:vec![],interactions:vec![],emitted_events:results,
        })
    }
    fn snapshot(&self,handle:NativeHandle)->ContractResult<Snapshot>{
        Ok(self.record(handle)?.snapshot.clone())
    }
    fn restore(&mut self,handle:NativeHandle,checkpoint:&Snapshot)->ContractResult<()>{
        let entity_id=self.record(handle)?.entity_id;
        if checkpoint.entity_id!=entity_id{
            return Err(invalid("cannot restore a foreign stage"));
        }
        let map=StageRecord::load_map(&checkpoint.state)?;
        let record=self.records.get_mut(&handle.native_slot)
            .ok_or(ContractError::NotFound(self.context_id))?;
        record.snapshot=checkpoint.clone();
        record.map=map;
        Ok(())
    }
    fn remove(&mut self,handle:NativeHandle)->ContractResult<()>{
        self.record(handle)?;
        self.records.remove(&handle.native_slot);
        Ok(())
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    use oasis_contracts::{
        FrameMap,GeometryRequest,GeometryResult,SpatialHit,SpatialQuery,
    };
    use oasis_runtime::{catalog::Catalog,Host};
    struct UnusedWorld;
    impl WorldPort for UnusedWorld{
        fn geometry(&mut self,_:GeometryRequest)->ContractResult<GeometryResult>{Err(invalid("unused"))}
        fn query(&mut self,_:SpatialQuery)->ContractResult<Vec<SpatialHit>>{Err(invalid("unused"))}
        fn frame_map(&self,_:Id,_:Id)->ContractResult<FrameMap>{Err(invalid("unused"))}
        fn entity_view(&self,_:Id)->ContractResult<EntityView>{Err(invalid("unused"))}
        fn submit_interaction(&mut self,_:oasis_contracts::InteractionRequest)
            ->ContractResult<()>{Err(invalid("unused"))}
    }
    fn native_pxm()->Vec<u8>{
        // Legal original PXM 0x10 format; no proprietary game asset required.
        let mut bytes=b"PXM".to_vec();
        bytes.push(0x10);
        bytes.extend_from_slice(&3u16.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&[1,0,2,2,1,0]);
        bytes
    }
    fn stage()->EntityView{
        let mut attributes=vec![0u8;256];
        attributes[1]=0x41;
        attributes[2]=0x62;
        EntityView{
            id:Id(0x70),
            definition:DefinitionRef{id:DEFINITION_ID,version:1},
            revision:oasis_contracts::Revision(5),
            origin_module:Some(MODULE_ID),
            components:vec![
                TypedValue{
                    type_ref:kind(STAGE_DATA),
                    data:Value::Map(BTreeMap::from([
                        ("pxm".into(),Value::Bytes(native_pxm())),
                        ("attributes".into(),Value::Bytes(attributes)),
                    ])),
                },
                TypedValue{
                    type_ref:TypeRef{
                        namespace:"source.unknown".into(),name:"metadata".into(),version:9,
                    },
                    data:Value::Bytes(vec![0,254,255]),
                },
            ],
        }
    }
    fn query(x:u64,y:u64)->InputIntent{
        InputIntent{
            controller_entity_id:Id(0x70),
            intent:TypedValue{
                type_ref:kind(TILE_QUERY),
                data:Value::Map(BTreeMap::from([
                    ("x".into(),Value::UInt(x)),
                    ("y".into(),Value::UInt(y)),
                ])),
            },
        }
    }
    fn read_result(value:&TypedValue,name:&str)->Value{
        map_value(&value.data).unwrap().get(name).unwrap().clone()
    }
    fn clock(tick:u64)->ClockStep{
        ClockStep{
            native_tick:tick,simulation_time_nanos:u128::from(tick)*20_000_000,
            delta_nanos:20_000_000,
        }
    }
    #[test]
    fn original_pxm_loader_and_tile_attribute_lookup_run_through_host(){
        let adapter=CaveStoryStageAdapter;
        let mut catalog=Catalog::new();
        catalog.install(&adapter).unwrap();
        assert_eq!(catalog.capability_provider(&kind(TILE_QUERY)),Some(MODULE_ID));
        let mut host=Host::new();
        host.register_module(Id(0x90),
            adapter.start_context(MODULE_ID,Id(0x90)).unwrap()).unwrap();
        let stage=stage();
        host.instantiate(Id(0x90),stage.clone(),None).unwrap();
        let results=host.step(Id(0x90),clock(1),&[
            query(0,0),query(2,0),query(0,1),query(99,0),
        ]).unwrap().emitted_events;
        assert_eq!(read_result(&results[0],"attribute"),Value::UInt(0x41));
        assert_eq!(read_result(&results[1],"attribute"),Value::UInt(0x62));
        assert_eq!(read_result(&results[2],"tile"),Value::UInt(2));
        assert_eq!(read_result(&results[3],"attribute"),Value::UInt(0));
        assert_eq!(read_result(&results[3],"in_bounds"),Value::Bool(false));
        assert_eq!(host.snapshot(stage.id).unwrap().state,stage.components);
    }
    #[test]
    fn native_map_and_unknown_source_state_survive_host_recovery(){
        let adapter=CaveStoryStageAdapter;
        let mut a=Host::new();
        let mut b=Host::new();
        a.register_module(Id(1),adapter.start_context(MODULE_ID,Id(1)).unwrap()).unwrap();
        b.register_module(Id(2),adapter.start_context(MODULE_ID,Id(2)).unwrap()).unwrap();
        let stage=stage();
        a.instantiate(Id(1),stage.clone(),None).unwrap();
        let saved=a.snapshot(stage.id).unwrap();
        a.remove(stage.id).unwrap();
        b.instantiate(Id(2),stage.clone(),Some(&saved)).unwrap();
        assert_eq!(b.snapshot(stage.id).unwrap().state,saved.state);
        let results=b.step(Id(2),clock(1),&[query(1,1)]).unwrap().emitted_events;
        assert_eq!(read_result(&results[0],"attribute"),Value::UInt(0x41));
    }
    #[test]
    fn invalid_pxm_and_foreign_snapshots_are_rejected(){
        let adapter=CaveStoryStageAdapter;
        let mut native=adapter.start_context(MODULE_ID,Id(33)).unwrap();
        let mut record=stage();
        let Value::Map(map)=&mut record.components[0].data else {panic!("bad fixture")};
        map.insert("pxm".into(),Value::Bytes(vec![1,2,3]));
        assert!(native.instantiate(&record,None).is_err());
        let good=stage();
        let handle=native.instantiate(&good,None).unwrap();
        let mut checkpoint=native.snapshot(handle).unwrap();
        checkpoint.entity_id=Id(99);
        assert!(native.restore(handle,&checkpoint).is_err());
        assert_eq!(native.snapshot(handle).unwrap().entity_id,good.id);
    }
}
