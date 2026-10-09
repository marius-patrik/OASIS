//! OASIS v0.1 contract sketch: compile-oriented Rust surface, not a runtime implementation.
//! No dependencies on game implementations are permitted in this crate.
//! IDs are abstract 128-bit values; the transport layer maps them to SQL UUIDs.

use std::collections::BTreeMap;
use serde::{Serialize, Deserialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Id(pub u128);

// JSON transports must not truncate global 128-bit object identities into
// floating-point values. Native ID representations stay decimal strings.
impl Serialize for Id {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok,S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}
impl<'de> Deserialize<'de> for Id {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self,D::Error> {
        let s=String::deserialize(deserializer)?;
        s.parse::<u128>().map(Id).map_err(serde::de::Error::custom)
    }
}

mod finite_f64 {
    use serde::{Deserialize,Deserializer,Serializer};
    pub fn serialize<S:Serializer>(value:&f64, serializer:S)->Result<S::Ok,S::Error>{
        if !value.is_finite(){
            return Err(serde::ser::Error::custom("nonfinite game-native float"));
        }
        serializer.serialize_f64(*value)
    }
    pub fn deserialize<'de,D:Deserializer<'de>>(deserializer:D)->Result<f64,D::Error>{
        let value=f64::deserialize(deserializer)?;
        if !value.is_finite(){
            return Err(serde::de::Error::custom("nonfinite game-native float"));
        }
        Ok(value)
    }
}
mod finite_f64_array4 {
    use serde::{Deserialize,Deserializer,Serializer};
    pub fn serialize<S:Serializer>(value:&[f64;4],serializer:S)->Result<S::Ok,S::Error>{
        if value.iter().any(|v|!v.is_finite()){
            return Err(serde::ser::Error::custom("nonfinite quaternion"));
        }
        serde::Serialize::serialize(value,serializer)
    }
    pub fn deserialize<'de,D:Deserializer<'de>>(deserializer:D)->Result<[f64;4],D::Error>{
        let values= <[f64;4]>::deserialize(deserializer)?;
        if values.iter().any(|v|!v.is_finite()){
            return Err(serde::de::Error::custom("nonfinite quaternion"));
        }
        Ok(values)
    }
}
mod finite_f64_array16 {
    use serde::{Deserialize,Deserializer,Serializer};
    pub fn serialize<S:Serializer>(value:&[f64;16],serializer:S)->Result<S::Ok,S::Error>{
        if value.iter().any(|v|!v.is_finite()){
            return Err(serde::ser::Error::custom("nonfinite spatial frame matrix"));
        }
        serde::Serialize::serialize(value,serializer)
    }
    pub fn deserialize<'de,D:Deserializer<'de>>(deserializer:D)->Result<[f64;16],D::Error>{
        let values= <[f64;16]>::deserialize(deserializer)?;
        if values.iter().any(|v|!v.is_finite()){
            return Err(serde::de::Error::custom("nonfinite spatial frame matrix"));
        }
        Ok(values)
    }
}

mod decimal_u128 {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(value: &u128, serializer: S)
        -> Result<S::Ok,S::Error> {
        serializer.serialize_str(&value.to_string())
    }
    pub fn deserialize<'de,D: Deserializer<'de>>(deserializer: D)
        -> Result<u128,D::Error> {
        let text=String::deserialize(deserializer)?;
        text.parse::<u128>().map_err(serde::de::Error::custom)
    }
}


#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TypeRef {
    pub namespace: String,
    pub name: String,
    pub version: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    UInt(u64),
    Float(#[serde(with = "finite_f64")] f64),
    String(String),
    Bytes(Vec<u8>),
    Ref(Id),
    Sequence(Vec<Value>),
    Map(BTreeMap<String, Value>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TypedValue {
    pub type_ref: TypeRef,
    pub data: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revision(pub u64);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DefinitionRef {
    pub id: Id,
    pub version: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NativeHandle {
    pub context_id: Id,
    pub native_slot: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntityView {
    pub id: Id,
    pub definition: DefinitionRef,
    pub revision: Revision,
    pub components: Vec<TypedValue>,
    pub origin_module: Option<Id>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vec3 {
    #[serde(with = "finite_f64")]
    pub x: f64,
    #[serde(with = "finite_f64")]
    pub y: f64,
    #[serde(with = "finite_f64")]
    pub z: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transform {
    pub frame_id: Id,
    pub position: Vec3,
    #[serde(with = "finite_f64_array4")]
    pub rotation_xyzw: [f64; 4],
    pub scale: Vec3,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FrameMap {
    pub source: Id,
    pub destination: Id,
    #[serde(with = "finite_f64_array16")]
    pub column_major_4x4: [f64; 16],
    #[serde(with = "finite_f64")]
    pub source_units_per_destination_unit: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Geometry {
    TriangleMesh { asset_id: Id, transform: Transform },
    Aabb { frame_id: Id, minimum: Vec3, maximum: Vec3 },
    Plane { frame_id: Id, normal: Vec3, distance: f64 },
    Custom(TypedValue),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeometryRequest {
    pub frame_id: Id,
    pub center: Vec3,
    #[serde(with = "finite_f64")]
    pub radius: f64,
    pub filter: Option<TypedValue>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeometryResult {
    pub shapes: Vec<Geometry>,
    pub revision: Revision,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpatialQuery {
    pub frame_id: Id,
    pub query: TypedValue,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpatialHit {
    pub target: Option<Id>,
    pub position: Vec3,
    pub normal: Vec3,
    #[serde(with = "finite_f64")]
    pub distance: f64,
    pub data: Option<TypedValue>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityStamp {
    pub resource_key: String,
    pub context_id: Id,
    pub epoch: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum InteractionTarget {
    Entity(Id),
    World(Id),
    Region { frame_id: Id, geometry: Geometry },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InteractionRequest {
    pub id: Id,
    pub source_entity_id: Id,
    pub target: InteractionTarget,
    pub operation: TypedValue,
    pub authority: AuthorityStamp,
    pub source_tick: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum InteractionDisposition {
    Applied { result: Option<TypedValue> },
    Rejected { reason: String },
    Deferred,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InteractionResult {
    pub request_id: Id,
    pub disposition: InteractionDisposition,
    pub effects: Vec<TypedValue>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ClockStep {
    pub native_tick: u64,
    #[serde(with = "decimal_u128")]
    pub simulation_time_nanos: u128,
    pub delta_nanos: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InputIntent {
    pub controller_entity_id: Id,
    pub intent: TypedValue,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModuleDescriptor {
    pub engine_id: Id,
    pub module_id: Id,
    pub contract_major: u32,
    pub contract_minor: u32,
    pub exported_interfaces: Vec<TypeRef>,
    pub required_interfaces: Vec<TypeRef>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ContractError {
    Unsupported { interface: TypeRef, reason: String },
    NotFound(Id),
    StaleRevision { expected: Revision, actual: Revision },
    StaleAuthority,
    InvalidData(String),
    Internal(String),
}

pub type ContractResult<T> = Result<T, ContractError>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub entity_id: Id,
    pub state: Vec<TypedValue>,
    pub revision: Revision,
    pub binary_artifact: Option<Id>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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
    #[serde(with = "finite_f64_array16")]
    pub view_column_major_4x4: [f64; 16],
    #[serde(with = "finite_f64_array16")]
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
    #[serde(with = "decimal_u128")]
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