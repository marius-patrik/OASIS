//! First independent, source-backed native Cave Story capability.
//!
//! Uses `doukutsu_rs::game::physics::HitExtents` directly, at the pinned
//! upstream revision. This is NOT a Cave Story game-loop, weapons, world,
//! controller, native rendering, or playable character adapter. Only original
//! hitbox point tests are implemented; OASIS supplies identity and transport.
//! No other game is referenced or imported.

pub mod weapon;
pub mod stage;
mod player;

use std::collections::BTreeMap;

use doukutsu_rs::game::physics::HitExtents;
use oasis_contracts::{
    AdapterRegistry, ClockStep, ContractError, ContractResult, DefinitionRef,
    EntityView, GameAdapter, Id, InputIntent, ModuleDescriptor, NativeHandle,
    NativeModule, Revision, Snapshot, StepOutput, TypeRef, TypedValue, Value,
    WorldPort,
};

pub const ENGINE_ID: Id = Id(0xc451_0001);
pub const MODULE_ID: Id = Id(0xc451_0002);
pub const DEFINITION_ID: Id = Id(0xc451_0003);

const SPACE: &str = "cave-story";
const HITBOX: &str = "hitbox";
const HIT_TEST: &str = "hit-test-point";
const HIT_RESULT: &str = "hit-test-result";

fn kind(name: &str) -> TypeRef {
    TypeRef { namespace: SPACE.into(), name: name.into(), version: 1 }
}
fn bad(message: &str) -> ContractError {
    ContractError::InvalidData(message.into())
}
fn values(value: &Value) -> ContractResult<&BTreeMap<String, Value>> {
    match value {
        Value::Map(v) => Ok(v),
        _ => Err(bad("native value must be a map")),
    }
}
fn coordinate(map: &BTreeMap<String, Value>, key: &str) -> ContractResult<i32> {
    match map.get(key) {
        Some(Value::Int(value)) => i32::try_from(*value)
            .map_err(|_| bad("native coordinate exceeds i32 range")),
        _ => Err(bad("native coordinate must be a signed integer")),
    }
}
fn extent(map: &BTreeMap<String, Value>, key: &str) -> ContractResult<u32> {
    match map.get(key) {
        Some(Value::UInt(value)) => u32::try_from(*value)
            .map_err(|_| bad("native hitbox extent exceeds u32 range")),
        _ => Err(bad("native hitbox extent must be an unsigned integer")),
    }
}

/// Native fixed-point units, not platform pixels or a reconstructed physics
/// model. The original Cave Story collision functions determine membership.
struct NativeCollider {
    x: i32,
    y: i32,
    bounds: HitExtents,
}
impl NativeCollider {
    fn load(state: &[TypedValue]) -> ContractResult<Self> {
        let component = state.iter().find(|part| part.type_ref == kind(HITBOX))
            .ok_or_else(|| bad("source-native Cave Story hitbox missing"))?;
        let fields = values(&component.data)?;
        Ok(Self {
            x: coordinate(fields, "x")?,
            y: coordinate(fields, "y")?,
            bounds: HitExtents {
                left: extent(fields, "left")?,
                right: extent(fields, "right")?,
                top: extent(fields, "top")?,
                bottom: extent(fields, "bottom")?,
            },
        })
    }

    fn test(&self, x: i32, y: i32) -> (bool, bool) {
        (
            self.bounds.point_in_entity_x(self.x, x),
            self.bounds.point_in_entity_y(self.y, y),
        )
    }
}

struct Instance {
    entity_id: Id,
    snapshot: Snapshot,
    collider: NativeCollider,
}

pub struct CaveStoryAdapter;

impl GameAdapter for CaveStoryAdapter {
    fn register(&self, registry: &mut dyn AdapterRegistry) -> ContractResult<()> {
        registry.register_module(ModuleDescriptor {
            engine_id: ENGINE_ID,
            module_id: MODULE_ID,
            contract_major: 0,
            contract_minor: 1,
            exported_interfaces: vec![kind(HIT_TEST)],
            required_interfaces: vec![],
        })?;
        registry.register_definition(DefinitionRef { id: DEFINITION_ID, version: 1 })?;
        registry.register_capability(kind(HIT_TEST), MODULE_ID)
    }

    fn start_context(&self, module_id: Id, context_id: Id)
        -> ContractResult<Box<dyn NativeModule>> {
        if module_id != MODULE_ID {
            return Err(ContractError::NotFound(module_id));
        }
        Ok(Box::new(CaveStoryNativeHitbox {
            context_id, next_slot: 0, last_tick: 0,
            instances: BTreeMap::new(),
        }))
    }
}

/// An actual independently instantiable OASIS module that delegates the
/// point-inside-native-hitbox semantics to the upstream game implementation.
pub struct CaveStoryNativeHitbox {
    context_id: Id,
    next_slot: u64,
    last_tick: u64,
    instances: BTreeMap<u64, Instance>,
}
impl CaveStoryNativeHitbox {
    fn instance(&self, handle: NativeHandle) -> ContractResult<&Instance> {
        if handle.context_id != self.context_id {
            return Err(bad("native handle belongs to another context"));
        }
        self.instances.get(&handle.native_slot)
            .ok_or(ContractError::NotFound(self.context_id))
    }
}

impl NativeModule for CaveStoryNativeHitbox {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor {
            engine_id: ENGINE_ID,
            module_id: MODULE_ID,
            contract_major: 0,
            contract_minor: 1,
            exported_interfaces: vec![kind(HIT_TEST)],
            required_interfaces: vec![],
        }
    }

    fn instantiate(&mut self, entity: &EntityView, saved: Option<&Snapshot>)
        -> ContractResult<NativeHandle> {
        if entity.origin_module != Some(MODULE_ID) {
            return Err(bad("entity does not originate in this native module"));
        }
        let snapshot = saved.cloned().unwrap_or_else(|| Snapshot {
            entity_id: entity.id, state: entity.components.clone(),
            revision: entity.revision,
            binary_artifact: None,
        });
        if snapshot.entity_id != entity.id {
            return Err(bad("foreign native snapshot"));
        }
        let collider = NativeCollider::load(&snapshot.state)?;
        if self.instances.values().any(|instance| instance.entity_id == entity.id) {
            return Err(bad("entity already instantiated"));
        }
        let next = self.next_slot.checked_add(1)
            .ok_or_else(|| bad("native handle counter exhausted"))?;
        self.next_slot = next;
        self.instances.insert(next, Instance { entity_id: entity.id, snapshot, collider });
        Ok(NativeHandle { context_id: self.context_id, native_slot: next })
    }

    fn step(&mut self, clock: ClockStep, inputs: &[InputIntent], _world: &mut dyn WorldPort)
        -> ContractResult<StepOutput> {
        if clock.native_tick <= self.last_tick {
            return Err(bad("native clock must advance"));
        }
        let mut events = Vec::new();
        for input in inputs {
            if input.intent.type_ref != kind(HIT_TEST) {
                return Err(ContractError::Unsupported {
                    interface: input.intent.type_ref.clone(),
                    reason: "native Cave Story hitbox module only tests points".into(),
                });
            }
            let query = values(&input.intent.data)?;
            let x = coordinate(query, "x")?;
            let y = coordinate(query, "y")?;
            let collider = self.instances.values()
                .find(|instance| instance.entity_id == input.controller_entity_id)
                .ok_or(ContractError::NotFound(input.controller_entity_id))?;
            let (inside_x, inside_y) = collider.collider.test(x, y);
            events.push(TypedValue {
                type_ref: kind(HIT_RESULT),
                data: Value::Map(BTreeMap::from([
                    ("entity".into(), Value::Ref(collider.entity_id)),
                    ("inside".into(), Value::Bool(inside_x && inside_y)),
                    ("inside_x".into(), Value::Bool(inside_x)),
                    ("inside_y".into(), Value::Bool(inside_y)),
                    ("point_x".into(), Value::Int(i64::from(x))),
                    ("point_y".into(), Value::Int(i64::from(y))),
                ])),
            });
        }
        self.last_tick = clock.native_tick;
        Ok(StepOutput {
            state_changes: vec![],
            interactions: vec![],
            emitted_events: events,
        })
    }

    fn snapshot(&self, handle: NativeHandle) -> ContractResult<Snapshot> {
        Ok(self.instance(handle)?.snapshot.clone())
    }

    fn restore(&mut self, handle: NativeHandle, snapshot: &Snapshot) -> ContractResult<()> {
        let entity_id = self.instance(handle)?.entity_id;
        if snapshot.entity_id != entity_id {
            return Err(bad("cannot restore a different entity"));
        }
        let collider = NativeCollider::load(&snapshot.state)?;
        let instance = self.instances.get_mut(&handle.native_slot)
            .ok_or(ContractError::NotFound(self.context_id))?;
        instance.snapshot = snapshot.clone();
        instance.collider = collider;
        Ok(())
    }

    fn remove(&mut self, handle: NativeHandle) -> ContractResult<()> {
        self.instance(handle)?;
        self.instances.remove(&handle.native_slot);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis_contracts::{
        FrameMap, GeometryRequest, GeometryResult, SpatialHit, SpatialQuery,
    };

    struct EmptyWorld;
    impl WorldPort for EmptyWorld {
        fn geometry(&mut self, _: GeometryRequest) -> ContractResult<GeometryResult> {
            Err(bad("unused"))
        }
        fn query(&mut self, _: SpatialQuery) -> ContractResult<Vec<SpatialHit>> {
            Err(bad("unused"))
        }
        fn frame_map(&self, _: Id, _: Id) -> ContractResult<FrameMap> {
            Err(bad("unused"))
        }
        fn entity_view(&self, _: Id) -> ContractResult<EntityView> {
            Err(bad("unused"))
        }
        fn submit_interaction(&mut self, _: oasis_contracts::InteractionRequest)
            -> ContractResult<()> {
            Err(bad("unused"))
        }
    }

    fn component() -> TypedValue {
        TypedValue {
            type_ref: kind(HITBOX),
            data: Value::Map(BTreeMap::from([
                ("x".into(),Value::Int(1000)),
                ("y".into(),Value::Int(2000)),
                ("left".into(),Value::UInt(20)),
                ("right".into(),Value::UInt(30)),
                ("top".into(),Value::UInt(50)),
                ("bottom".into(),Value::UInt(60)),
            ])),
        }
    }
    fn entity() -> EntityView {
        EntityView {
            id: Id(81),
            definition: DefinitionRef { id: DEFINITION_ID, version: 1 },
            revision: Revision(9),
            components: vec![component()],
            origin_module: Some(MODULE_ID),
        }
    }
    fn query(x: i64, y: i64) -> InputIntent {
        InputIntent {
            controller_entity_id: Id(81),
            intent: TypedValue {
                type_ref: kind(HIT_TEST),
                data: Value::Map(BTreeMap::from([
                    ("x".into(),Value::Int(x)),("y".into(),Value::Int(y)),
                ])),
            },
        }
    }
    fn verdict(event: &TypedValue) -> bool {
        match &event.data {
            Value::Map(data) => matches!(data.get("inside"),Some(Value::Bool(true))),
            _ => false,
        }
    }
    fn tick(n: u64) -> ClockStep {
        ClockStep { native_tick:n, simulation_time_nanos:u128::from(n)*20_000_000,
            delta_nanos:20_000_000 }
    }

    #[test]
    fn executes_upstream_cave_story_exclusive_collision_semantics() {
        let adapter=CaveStoryAdapter;
        let mut native=adapter.start_context(MODULE_ID,Id(101)).unwrap();
        let player=entity();
        let handle=native.instantiate(&player,None).unwrap();
        let mut world=EmptyWorld;
        // Right/left and top/bottom edges are EXCLUSIVE in source engine.
        // No reimplementation of that algorithm lives in this adapter.
        for (n,x,y,expected) in [
            (1,1000,2000,true),
            (2,980,2000,false),
            (3,1030,2000,false),
            (4,1000,1950,false),
            (5,1000,2060,false),
            (6,981,1951,true),
            (7,1029,2059,true),
        ] {
            let output=native.step(tick(n),&[query(x,y)],&mut world).unwrap();
            assert_eq!(output.emitted_events.len(),1);
            assert_eq!(verdict(&output.emitted_events[0]),expected);
            assert!(output.state_changes.is_empty());
        }
        assert_eq!(native.snapshot(handle).unwrap().revision,Revision(9));
    }

    #[test]
    fn snapshot_roundtrip_preserves_unknown_native_components() {
        let adapter=CaveStoryAdapter;
        let mut a=adapter.start_context(MODULE_ID,Id(101)).unwrap();
        let mut b=adapter.start_context(MODULE_ID,Id(202)).unwrap();
        let mut player=entity();
        player.components.push(TypedValue {
            type_ref: TypeRef { namespace:"arbitrary".into(),
                name:"native-secret".into(), version:7 },
            data: Value::Bytes(vec![0,1,254,255]),
        });
        let first=a.instantiate(&player,None).unwrap();
        let saved=a.snapshot(first).unwrap();
        assert_eq!(saved.state.len(),2);
        a.remove(first).unwrap();
        assert!(a.snapshot(first).is_err());
        let second=b.instantiate(&player,Some(&saved)).unwrap();
        let restored=b.snapshot(second).unwrap();
        assert_eq!(saved.state,restored.state);
        assert_eq!(saved.revision,restored.revision);
        assert_ne!(first.context_id,second.context_id);
        assert!(b.snapshot(first).is_err());

        let mut corrupt=saved.clone();
        corrupt.entity_id=Id(88);
        assert!(b.restore(second,&corrupt).is_err());
        assert_eq!(b.snapshot(second).unwrap().state,saved.state);
    }

    #[test]
    fn rejects_invalid_source_native_bounds_and_foreign_modules() {
        let adapter=CaveStoryAdapter;
        assert!(adapter.start_context(Id(0),Id(1)).is_err());
        let mut native=adapter.start_context(MODULE_ID,Id(1)).unwrap();
        let mut player=entity();
        player.origin_module=Some(Id(999));
        assert!(native.instantiate(&player,None).is_err());
        player=entity();
        player.components[0].data=Value::Map(BTreeMap::from([
            ("x".into(),Value::Float(1.5)),
        ]));
        assert!(native.instantiate(&player,None).is_err());
    }
    #[test]
    fn real_native_capability_executes_through_universal_oasis_host() {
        use oasis_runtime::{Host, catalog::Catalog};

        let adapter=CaveStoryAdapter;
        let mut catalog=Catalog::new();
        catalog.install(&adapter).unwrap();
        assert!(catalog.module(MODULE_ID).is_some());
        assert_eq!(catalog.capability_provider(&kind(HIT_TEST)),Some(MODULE_ID));

        let mut first=Host::new();
        let context=Id(501);
        first.register_module(context,
            adapter.start_context(MODULE_ID,context).unwrap()).unwrap();
        let entity=entity();
        first.instantiate(context,entity.clone(),None).unwrap();
        let events=first.step(context,tick(1),&[query(1000,2000)])
            .unwrap().emitted_events;
        assert!(verdict(&events[0]));
        let snapshot=first.snapshot(entity.id).unwrap();

        // Another host simulates the same originating module independently.
        // Transfer its original source-native state, not a translated proxy.
        first.remove(entity.id).unwrap();
        let mut second=Host::new();
        second.register_module(Id(502),
            adapter.start_context(MODULE_ID,Id(502)).unwrap()).unwrap();
        second.instantiate(Id(502),entity.clone(),Some(&snapshot)).unwrap();
        assert_eq!(second.snapshot(entity.id).unwrap().state,snapshot.state);
        let events=second.step(Id(502),tick(1),&[query(980,2000)])
            .unwrap().emitted_events;
        assert!(!verdict(&events[0]));
    }

}
