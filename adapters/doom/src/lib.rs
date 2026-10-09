//! Standalone, source-backed DOOM native math and map geometry routines.
//! Original functions are called from the pinned Rust port `room`; this is
//! NOT a full DOOM simulation/game-loop, player controller or renderer.
//! No other game or game-pair assumptions appear in this crate.

use std::collections::BTreeMap;

use oasis_contracts::{
    AdapterRegistry, ClockStep, ContractError, ContractResult, DefinitionRef,
    EntityView, GameAdapter, Id, InputIntent, ModuleDescriptor, NativeHandle,
    NativeModule, Snapshot, StepOutput, TypeRef, TypedValue, Value, WorldPort,
};
use room::doom::{
    m_bbox::{BBox, M_AddToBox, M_ClearBox},
    m_fixed::{FixedDiv, FixedMul},
    p_maputl::P_AproxDistance,
};

pub const ENGINE_ID: Id = Id(0xd000_0001);
pub const MODULE_ID: Id = Id(0xd000_0002);
pub const DEFINITION_ID: Id = Id(0xd000_0003);

fn kind(name: &str) -> TypeRef {
    TypeRef { namespace: "doom".into(), name: name.into(), version: 1 }
}
fn bad(reason: &str) -> ContractError {
    ContractError::InvalidData(reason.into())
}
fn fields(value: &Value) -> ContractResult<&BTreeMap<String, Value>> {
    match value {
        Value::Map(map) => Ok(map),
        _ => Err(bad("original DOOM operation requires a native field map")),
    }
}
fn fixed(map: &BTreeMap<String, Value>, key: &str) -> ContractResult<i32> {
    match map.get(key) {
        Some(Value::Int(v)) => i32::try_from(*v)
            .map_err(|_| bad("DOOM 16.16 fixed-point value exceeds 32-bit range")),
        _ => Err(bad("DOOM fixed-point value must be a signed integer")),
    }
}

const MUL: &str = "fixed-mul";
const DIV: &str = "fixed-div";
const DISTANCE: &str = "approx-distance";
const BOX: &str = "bbox-from-points";
const RESULT: &str = "native-result";

pub struct DoomMathAdapter;
impl GameAdapter for DoomMathAdapter {
    fn register(&self, registry: &mut dyn AdapterRegistry) -> ContractResult<()> {
        registry.register_module(ModuleDescriptor {
            engine_id: ENGINE_ID,module_id: MODULE_ID,
            contract_major: 0,contract_minor: 1,
            exported_interfaces: vec![kind(MUL),kind(DIV),kind(DISTANCE),kind(BOX)],
            required_interfaces: vec![],
        })?;
        registry.register_definition(DefinitionRef{id:DEFINITION_ID,version:1})?;
        for name in [MUL,DIV,DISTANCE,BOX] {
            registry.register_capability(kind(name),MODULE_ID)?;
        }
        Ok(())
    }
    fn start_context(&self,module:Id,context:Id)->ContractResult<Box<dyn NativeModule>>{
        if module!=MODULE_ID{return Err(ContractError::NotFound(module));}
        Ok(Box::new(DoomNativeMath{
            context,next_slot:0,last_tick:0,instances:BTreeMap::new(),
        }))
    }
}

struct NativeInstance {
    entity_id: Id,
    state: Snapshot,
}
pub struct DoomNativeMath {
    context: Id,
    next_slot: u64,
    last_tick: u64,
    instances: BTreeMap<u64,NativeInstance>,
}
impl DoomNativeMath {
    fn instance(&self,handle:NativeHandle)->ContractResult<&NativeInstance>{
        if handle.context_id!=self.context{return Err(ContractError::StaleAuthority);}
        self.instances.get(&handle.native_slot)
            .ok_or(ContractError::NotFound(self.context))
    }
    fn compute(intent:&TypedValue)->ContractResult<Value>{
        let name=intent.type_ref.name.as_str();
        if intent.type_ref.namespace!="doom" || intent.type_ref.version!=1
            || ![MUL,DIV,DISTANCE,BOX].contains(&name)
        {
            return Err(ContractError::Unsupported{
                interface:intent.type_ref.clone(),
                reason:"unrecognized original DOOM native operation".into(),
            });
        }
        let input=fields(&intent.data)?;
        if name==BOX {
            let Some(Value::Sequence(points))=input.get("points") else{
                return Err(bad("bbox requires a nonempty source-native point sequence"));
            };
            if points.is_empty(){return Err(bad("bbox must contain points"));}
            if points.len()>65_536{return Err(bad("too many native bbox points"));}
            let mut parsed=Vec::with_capacity(points.len());
            for point in points {
                let m=fields(point)?;
                parsed.push((fixed(m,"x")?,fixed(m,"y")?));
            }
            // Only these original source routines define the collision box
            // ordering and edge semantics; the adapter copies no formulas.
            let mut bbox=[0i32;4];
            // SAFETY: each function receives a valid pointer to four mutable
            // fixed_t slots and never stores that pointer after returning.
            unsafe {
                M_ClearBox(bbox.as_mut_ptr());
                for (x,y) in parsed {M_AddToBox(bbox.as_mut_ptr(),x,y);}
            }
            return Ok(Value::Map(BTreeMap::from([
                ("top".into(),Value::Int(i64::from(bbox[BBox::TOP]))),
                ("bottom".into(),Value::Int(i64::from(bbox[BBox::BOTTOM]))),
                ("left".into(),Value::Int(i64::from(bbox[BBox::LEFT]))),
                ("right".into(),Value::Int(i64::from(bbox[BBox::RIGHT]))),
            ])));
        }
        let a=fixed(input,"a")?;
        let b=fixed(input,"b")?;
        // This one operand pair is undefined in the upstream C/Rust
        // FixedDiv implementation (signed abs of INT_MIN divided by zero).
        if name==DIV && a==i32::MIN && b==0{
            return Err(bad("undefined source-native DOOM division"));
        }
        let v=match name{
            MUL=>FixedMul(a,b),
            DIV=>FixedDiv(a,b),
            DISTANCE=>P_AproxDistance(a,b),
            _=>return Err(bad("unknown DOOM native arithmetic")),
        };
        Ok(Value::Int(i64::from(v)))
    }
}
impl NativeModule for DoomNativeMath {
    fn descriptor(&self)->ModuleDescriptor{
        ModuleDescriptor{
            engine_id:ENGINE_ID,module_id:MODULE_ID,
            contract_major:0,contract_minor:1,
            exported_interfaces:vec![kind(MUL),kind(DIV),kind(DISTANCE),kind(BOX)],
            required_interfaces:vec![],
        }
    }
    fn instantiate(&mut self,entity:&EntityView,saved:Option<&Snapshot>)
        ->ContractResult<NativeHandle>{
        if entity.origin_module!=Some(MODULE_ID){
            return Err(bad("entity does not originate in the DOOM math module"));
        }
        let state=saved.cloned().unwrap_or_else(||Snapshot{
            entity_id:entity.id,state:entity.components.clone(),
            revision:entity.revision,binary_artifact:None,
        });
        if state.entity_id!=entity.id{return Err(bad("foreign DOOM state snapshot"));}
        if self.instances.values().any(|value|value.entity_id==entity.id){
            return Err(bad("native DOOM entity already instantiated"));
        }
        let next=self.next_slot.checked_add(1)
            .ok_or_else(||bad("native slot exhausted"))?;
        self.next_slot=next;
        self.instances.insert(next,NativeInstance{entity_id:entity.id,state});
        Ok(NativeHandle{context_id:self.context,native_slot:next})
    }
    fn step(&mut self,clock:ClockStep,inputs:&[InputIntent],_world:&mut dyn WorldPort)
        ->ContractResult<StepOutput>{
        if clock.native_tick<=self.last_tick{return Err(bad("native clock must advance"));}
        let mut events=Vec::with_capacity(inputs.len());
        for input in inputs {
            if !self.instances.values().any(|p|p.entity_id==input.controller_entity_id){
                return Err(ContractError::NotFound(input.controller_entity_id));
            }
            let value=Self::compute(&input.intent)?;
            events.push(TypedValue{
                type_ref:kind(RESULT),
                data:Value::Map(BTreeMap::from([
                    ("entity".into(),Value::Ref(input.controller_entity_id)),
                    ("result".into(),value),
                ])),
            });
        }
        self.last_tick=clock.native_tick;
        Ok(StepOutput{
            state_changes:vec![],interactions:vec![],emitted_events:events,
        })
    }
    fn snapshot(&self,handle:NativeHandle)->ContractResult<Snapshot>{
        Ok(self.instance(handle)?.state.clone())
    }
    fn restore(&mut self,handle:NativeHandle,snapshot:&Snapshot)->ContractResult<()>{
        let id=self.instance(handle)?.entity_id;
        if id!=snapshot.entity_id{return Err(bad("foreign DOOM checkpoint"));}
        self.instances.get_mut(&handle.native_slot)
            .ok_or(ContractError::NotFound(self.context))?
            .state=snapshot.clone();
        Ok(())
    }
    fn remove(&mut self,handle:NativeHandle)->ContractResult<()>{
        self.instance(handle)?;
        self.instances.remove(&handle.native_slot);
        Ok(())
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    use oasis_runtime::{catalog::Catalog,Host};

    fn entity()->EntityView {
        EntityView{
            id:Id(0xd0),definition:DefinitionRef{id:DEFINITION_ID,version:1},
            revision:oasis_contracts::Revision(7),
            origin_module:Some(MODULE_ID),
            components:vec![TypedValue{
                type_ref:TypeRef{
                    namespace:"native.doom".into(),name:"opaque".into(),version:5,
                },
                data:Value::Bytes(vec![0,1,254,255]),
            }],
        }
    }
    fn pair(op:&str,a:i32,b:i32)->InputIntent{
        InputIntent{
            controller_entity_id:Id(0xd0),
            intent:TypedValue{
                type_ref:kind(op),
                data:Value::Map(BTreeMap::from([
                    ("a".into(),Value::Int(i64::from(a))),
                    ("b".into(),Value::Int(i64::from(b))),
                ])),
            },
        }
    }
    fn bbox(points:&[(i32,i32)])->InputIntent{
        InputIntent{
            controller_entity_id:Id(0xd0),
            intent:TypedValue{
                type_ref:kind(BOX),
                data:Value::Map(BTreeMap::from([
                    ("points".into(),Value::Sequence(points.iter().map(|(x,y)|
                        Value::Map(BTreeMap::from([
                            ("x".into(),Value::Int(i64::from(*x))),
                            ("y".into(),Value::Int(i64::from(*y))),
                        ]))
                    ).collect())),
                ])),
            },
        }
    }
    fn value(event:&TypedValue)->&Value{
        let Value::Map(map)=&event.data else{panic!("not native result")};
        map.get("result").unwrap()
    }
    fn clock(n:u64)->ClockStep {
        ClockStep{
            native_tick:n,simulation_time_nanos:u128::from(n)*28_571_428,
            delta_nanos:28_571_428,
        }
    }

    #[test]
    fn executes_original_fixed_point_math_and_approx_distance_through_oasis(){
        let adapter=DoomMathAdapter;
        let mut catalog=Catalog::new();
        catalog.install(&adapter).unwrap();
        for name in [MUL,DIV,DISTANCE,BOX]{
            assert_eq!(catalog.capability_provider(&kind(name)),Some(MODULE_ID));
        }
        let mut host=Host::new();
        host.register_module(Id(10),
            adapter.start_context(MODULE_ID,Id(10)).unwrap()).unwrap();
        let entity=entity();
        host.instantiate(Id(10),entity.clone(),None).unwrap();
        let result=host.step(Id(10),clock(1),&[
            pair(MUL,3<<15,2<<16), // 1.5 x 2.0
            pair(DIV,3<<16,2<<16), // 3.0 / 2.0
            pair(DISTANCE,3<<16,4<<16), // original DOOM approximate
            bbox(&[(10,20),(30,5)]),
        ]).unwrap().emitted_events;
        assert_eq!(value(&result[0]),&Value::Int(i64::from(3<<16)));
        assert_eq!(value(&result[1]),&Value::Int(i64::from(3<<15)));
        assert_eq!(value(&result[2]),&Value::Int(i64::from(11<<15)));
        assert_eq!(value(&result[3]),&Value::Map(BTreeMap::from([
            ("top".into(),Value::Int(20)),
            ("bottom".into(),Value::Int(5)),
            ("left".into(),Value::Int(10)),
            ("right".into(),Value::Int(30)),
        ])));
        assert_eq!(host.snapshot(entity.id).unwrap().state,entity.components);
    }
    #[test]
    fn native_context_state_is_independent_and_restorable(){
        let adapter=DoomMathAdapter;
        let entity=entity();
        let mut first=Host::new();
        first.register_module(Id(10),
            adapter.start_context(MODULE_ID,Id(10)).unwrap()).unwrap();
        first.instantiate(Id(10),entity.clone(),None).unwrap();
        let snapshot=first.snapshot(entity.id).unwrap();
        first.remove(entity.id).unwrap();
        let mut second=Host::new();
        second.register_module(Id(20),
            adapter.start_context(MODULE_ID,Id(20)).unwrap()).unwrap();
        second.instantiate(Id(20),entity.clone(),Some(&snapshot)).unwrap();
        assert_eq!(second.snapshot(entity.id).unwrap().state,snapshot.state);
        let mut corrupt=snapshot.clone();
        corrupt.entity_id=Id(0);
        assert!(second.restore(entity.id,&corrupt).is_err());
        assert_eq!(second.snapshot(entity.id).unwrap().state,snapshot.state);
    }
    #[test]
    fn validates_native_types_and_preserves_upstream_saturation(){
        let adapter=DoomMathAdapter;
        let mut module=adapter.start_context(MODULE_ID,Id(12)).unwrap();
        let id=module.instantiate(&entity(),None).unwrap();
        struct Unused;
        impl WorldPort for Unused {
            fn geometry(&mut self,_:oasis_contracts::GeometryRequest)
                ->ContractResult<oasis_contracts::GeometryResult>{Err(bad("unused"))}
            fn query(&mut self,_:oasis_contracts::SpatialQuery)
                ->ContractResult<Vec<oasis_contracts::SpatialHit>>{Err(bad("unused"))}
            fn frame_map(&self,_:Id,_:Id)->ContractResult<oasis_contracts::FrameMap>
                {Err(bad("unused"))}
            fn entity_view(&self,_:Id)->ContractResult<EntityView>{Err(bad("unused"))}
            fn submit_interaction(&mut self,_:oasis_contracts::InteractionRequest)
                ->ContractResult<()>{Err(bad("unused"))}
        }
        let output=module.step(clock(1),&[
            pair(DIV,i32::MAX,1),
            pair(DIV,i32::MAX,-1),
        ],&mut Unused).unwrap();
        assert_eq!(value(&output.emitted_events[0]),&Value::Int(i64::from(i32::MAX)));
        assert_eq!(value(&output.emitted_events[1]),&Value::Int(i64::from(i32::MIN)));
        assert!(module.step(clock(2),&[pair(DIV,i32::MIN,0)],&mut Unused).is_err());
        assert_eq!(module.snapshot(id).unwrap().entity_id,Id(0xd0));
    }
}
