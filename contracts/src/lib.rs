//! OASIS v0.1 contract sketch: compile-oriented Rust surface, not a runtime implementation.
//! No dependencies on game implementations are permitted in this crate.
//! IDs are abstract 128-bit values; the transport layer maps them to SQL UUIDs.

use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Id(pub u128);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TypeRef {
    pub namespace: String,
    pub name: String,
    pub version: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    UInt(u64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    Ref(Id),
    Sequence(Vec<Value>),
    Map(BTreeMap<String, Value>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct TypedValue {
    pub type_ref: TypeRef,
    pub data: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Revision(pub u64);

#[derive(Clone, Debug)]
pub struct DefinitionRef {
    pub id: Id,
    pub version: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NativeHandle {
    pub context_id: Id,
    pub native_slot: u64,
}

#[derive(Clone, Debug)]
pub struct EntityView {
    pub id: Id,
    pub definition: DefinitionRef,
    pub revision: Revision,
    pub components: Vec<TypedValue>,
    pub origin_module: Option<Id>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Clone, Debug)]
pub struct Transform {
    pub frame_id: Id,
    pub position: Vec3,
    pub rotation_xyzw: [f64; 4],
    pub scale: Vec3,
}

#[derive(Clone, Debug)]
pub struct FrameMap {
    pub source: Id,
    pub destination: Id,
    pub column_major_4x4: [f64; 16],
    pub source_units_per_destination_unit: f64,
}

#[derive(Clone, Debug)]
pub enum Geometry {
    TriangleMesh { asset_id: Id, transform: Transform },
    Aabb { frame_id: Id, minimum: Vec3, maximum: Vec3 },
    Plane { frame_id: Id, normal: Vec3, distance: f64 },
    Custom(TypedValue),
}

#[derive(Clone, Debug)]
pub struct GeometryRequest {
    pub frame_id: Id,
    pub center: Vec3,
    pub radius: f64,
    pub filter: Option<TypedValue>,
}

#[derive(Clone, Debug)]
pub struct GeometryResult {
    pub shapes: Vec<Geometry>,
    pub revision: Revision,
}

#[derive(Clone, Debug)]
pub struct SpatialQuery {
    pub frame_id: Id,
    pub query: TypedValue,
}

#[derive(Clone, Debug)]
pub struct SpatialHit {
    pub target: Option<Id>,
    pub position: Vec3,
    pub normal: Vec3,
    pub distance: f64,
    pub data: Option<TypedValue>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorityStamp {
    pub resource_key: String,
    pub context_id: Id,
    pub epoch: u64,
}

#[derive(Clone, Debug)]
pub enum InteractionTarget {
    Entity(Id),
    World(Id),
    Region { frame_id: Id, geometry: Geometry },
}

#[derive(Clone, Debug)]
pub struct InteractionRequest {
    pub id: Id,
    pub source_entity_id: Id,
    pub target: InteractionTarget,
    pub operation: TypedValue,
    pub authority: AuthorityStamp,
    pub source_tick: u64,
}

#[derive(Clone, Debug)]
pub enum InteractionDisposition {
    Applied { result: Option<TypedValue> },
    Rejected { reason: String },
    Deferred,
}

#[derive(Clone, Debug)]
pub struct InteractionResult {
    pub request_id: Id,
    pub disposition: InteractionDisposition,
    pub effects: Vec<TypedValue>,
}

#[derive(Clone, Copy, Debug)]
pub struct ClockStep {
    pub native_tick: u64,
    pub simulation_time_nanos: u128,
    pub delta_nanos: u64,
}

#[derive(Clone, Debug)]
pub struct InputIntent {
    pub controller_entity_id: Id,
    pub intent: TypedValue,
}

#[derive(Clone, Debug)]
pub struct ModuleDescriptor {
    pub engine_id: Id,
    pub module_id: Id,
    pub contract_major: u32,
    pub contract_minor: u32,
    pub exported_interfaces: Vec<TypeRef>,
    pub required_interfaces: Vec<TypeRef>,
}

#[derive(Clone, Debug)]
pub enum ContractError {
    Unsupported { interface: TypeRef, reason: String },
    NotFound(Id),
    StaleRevision { expected: Revision, actual: Revision },
    StaleAuthority,
    InvalidData(String),
    Internal(String),
}

pub type ContractResult<T> = Result<T, ContractError>;

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub entity_id: Id,
    pub state: Vec<TypedValue>,
    pub revision: Revision,
    pub binary_artifact: Option<Id>,
}

#[derive(Clone, Debug)]
pub struct StepOutput {
    pub state_changes: Vec<Snapshot>,
    pub interactions: Vec<InteractionRequest>,
    pub emitted_events: Vec<TypedValue>,
}

/// The platform-provided, engine-agnostic view of a world.
/// World geometry is queried by native physics; it does not dictate that physics.
pub trait WorldPort {
    fn geometry(&mut self, request: GeometryRequest) -> ContractResult<GeometryResult>;
    fn query(&mut self, request: SpatialQuery) -> ContractResult<Vec<SpatialHit>>;
    fn frame_map(&self, source: Id, destination: Id) -> ContractResult<FrameMap>;
    fn entity_view(&self, entity_id: Id) -> ContractResult<EntityView>;
    fn submit_interaction(&mut self, request: InteractionRequest) -> ContractResult<()>;
}

/// Called by the platform with the result from the authoritative receiver.
pub trait InteractionReceiver {
    fn apply_interaction(
        &mut self,
        request: &InteractionRequest,
        world: &mut dyn WorldPort,
    ) -> ContractResult<InteractionResult>;
}

/// Independently instantiable native game logic. One adapter per game; no pair imports.
pub trait NativeModule: Send {
    fn descriptor(&self) -> ModuleDescriptor;
    fn instantiate(&mut self, entity: &EntityView, state: Option<&Snapshot>)
        -> ContractResult<NativeHandle>;
    fn step(
        &mut self,
        clock: ClockStep,
        inputs: &[InputIntent],
        world: &mut dyn WorldPort,
    ) -> ContractResult<StepOutput>;
    fn snapshot(&self, handle: NativeHandle) -> ContractResult<Snapshot>;
    fn restore(&mut self, handle: NativeHandle, snapshot: &Snapshot) -> ContractResult<()>;
    fn remove(&mut self, handle: NativeHandle) -> ContractResult<()>;
}

#[derive(Clone, Debug)]
pub struct CameraState {
    pub owner_entity_id: Id,
    pub frame_id: Id,
    pub view_column_major_4x4: [f64; 16],
    pub projection_column_major_4x4: [f64; 16],
    pub data: Option<TypedValue>,
}

#[derive(Clone, Debug)]
pub struct RenderFrame {
    pub world_instance_id: Id,
    pub frame_number: u64,
    pub camera: CameraState,
    pub width: u32,
    pub height: u32,
    pub output_time_nanos: u128,
}

#[derive(Clone, Debug)]
pub enum RenderContribution {
    Geometry { assets: Vec<Id>, transforms: Vec<Transform> },
    SharedSurface {
        resource_handle: String,
        depth_handle: Option<String>,
        frame_number: u64,
    },
    Overlay { resource_handle: String, order: i32 },
}

/// Renderer implementations can remain native to the source engine.
pub trait RenderProvider: Send {
    fn camera(&mut self, controller: Id, world: &mut dyn WorldPort)
        -> ContractResult<CameraState>;
    fn contribute(&mut self, frame: &RenderFrame)
        -> ContractResult<Vec<RenderContribution>>;
}

/// Concrete adapter implementations register independently against this host interface.
pub trait AdapterRegistry {
    fn register_module(&mut self, descriptor: ModuleDescriptor) -> ContractResult<()>;
    fn register_definition(&mut self, definition: DefinitionRef) -> ContractResult<()>;
    fn register_capability(&mut self, capability: TypeRef, module_id: Id)
        -> ContractResult<()>;
}

pub trait GameAdapter {
    fn register(&self, registry: &mut dyn AdapterRegistry) -> ContractResult<()>;
    fn start_context(&self, module_id: Id, context_id: Id)
        -> ContractResult<Box<dyn NativeModule>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_handles_are_scoped_to_execution_contexts() {
        let a = NativeHandle { context_id: Id(10), native_slot: 1 };
        let b = NativeHandle { context_id: Id(11), native_slot: 1 };
        assert_ne!(a, b);
    }

    #[test]
    fn type_refs_are_versioned_and_namespaced() {
        let a = TypeRef { namespace: "core".into(), name: "impact".into(), version: 1 };
        let b = TypeRef { version: 2, ..a.clone() };
        assert_ne!(a, b);
    }
}