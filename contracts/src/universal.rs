//! Production-directed, source-neutral G0 full-game interfaces (contract major 1).
//! Existing partial source helpers are NOT automatically full-game conformant.
//! Native objects may be !Send: isolation is enforced by a worker process.

use serde::{Deserialize, Serialize};

use crate::{
    AuthorityStamp, CameraState, ClockStep, ContractError, ContractResult, EntityView,
    Geometry, GeometryRequest, Id, InputIntent, InteractionRequest, InteractionResult,
    NativeHandle, RenderFrame, Revision, StepOutput, TypeRef, TypedValue, WorldPort,
};

pub const FULL_GAME_CONTRACT_MAJOR: u32 = 1;

/// A native slot has meaning only within one execution context. A context is
/// bound to one world instance, one exact module artifact and one process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextKey {
    pub world_instance_id: Id,
    pub context_id: Id,
    pub module_id: Id,
}

impl ContextKey {
    pub fn validate(self) -> ContractResult<()> {
        if [self.world_instance_id, self.context_id, self.module_id]
            .iter().any(|id| id.0 == 0)
        {
            return Err(ContractError::InvalidData("zero context identity".into()));
        }
        Ok(())
    }

    pub fn check_handle(self, handle: NativeHandle) -> ContractResult<()> {
        self.validate()?;
        if handle.context_id != self.context_id {
            return Err(ContractError::StaleAuthority);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContextRole {
    World,
    Character,
    Item,
    Renderer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IsolationMode {
    /// A fresh address space: no static/global game state shared with peers.
    Process,
}

/// Worker identity is verified against the registered artifact at handshake.
/// This is an isolation description, not a claim of OS sandboxing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextManifest {
    pub key: ContextKey,
    pub role: ContextRole,
    /// Optional exclusive entity binding. None permits multiple native handles
    /// in a context; the trusted host must then check handle-to-entity bindings.
    pub owner_entity_id: Option<Id>,
    pub isolation: IsolationMode,
    pub artifact_sha256: [u8; 32],
    /// Exact rational native ticks per second (avoid float-rate drift).
    pub ticks_per_second_num: u32,
    pub ticks_per_second_den: u32,
}

impl ContextManifest {
    pub fn validate(&self) -> ContractResult<()> {
        self.key.validate()?;
        let binding_ok = match self.role {
            ContextRole::World => self.owner_entity_id.is_none(),
            ContextRole::Character | ContextRole::Item => {
                self.owner_entity_id.is_none_or(|id| id.0 != 0)
            }
            ContextRole::Renderer => self.owner_entity_id.is_none_or(|id| id.0 != 0),
        };
        if !binding_ok {
            return Err(ContractError::InvalidData("invalid context entity binding".into()));
        }
        if self.artifact_sha256 == [0; 32] {
            return Err(ContractError::InvalidData("missing artifact digest".into()));
        }
        if self.ticks_per_second_num == 0 || self.ticks_per_second_den == 0 {
            return Err(ContractError::InvalidData("invalid native tick rate".into()));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecyclePhase {
    Created,
    Initialized,
    Running,
    Suspended,
    Destroyed,
}

impl LifecyclePhase {
    /// A checkpoint is accepted only while simulation is quiesced.
    pub fn transition(self, next: Self) -> ContractResult<Self> {
        use LifecyclePhase::{Created, Destroyed, Initialized, Running, Suspended};
        let allowed = matches!(
            (self, next),
            (Created, Initialized)
                | (Initialized, Running)
                | (Running, Suspended)
                | (Suspended, Running)
                | (Initialized, Destroyed)
                | (Suspended, Destroyed)
        );
        if allowed { Ok(next) } else {
            Err(ContractError::InvalidData("illegal native lifecycle transition".into()))
        }
    }

    pub fn can_checkpoint(self) -> bool {
        matches!(self, Self::Initialized | Self::Suspended)
    }
}

/// Native state is opaque to OASIS. Only the source module may interpret it.
/// Completeness MUST be proven by terminate/restore/replay equivalence tests,
/// not by a boolean, metadata manifest or hash alone.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CheckpointScope {
    World(Id),
    Entity(Id),
    /// Renderer/source camera context state that is neither an entity nor world.
    Context(Id),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NativeCheckpoint {
    pub context: ContextKey,
    pub scope: CheckpointScope,
    pub native_codec: TypeRef,
    pub artifact_sha256: [u8; 32],
    pub native_tick: u64,
    pub state_revision: Revision,
    pub native_bytes: Vec<u8>,
    /// Independently registered typed platform values; not a replacement
    /// for full opaque source-owned simulation state.
    pub attached_components: Vec<TypedValue>,
}

impl NativeCheckpoint {
    pub fn validate_for(&self, manifest: &ContextManifest) -> ContractResult<()> {
        manifest.validate()?;
        if self.context != manifest.key || self.artifact_sha256 != manifest.artifact_sha256 {
            return Err(ContractError::StaleAuthority);
        }
        if self.native_codec.namespace.is_empty()
            || self.native_codec.name.is_empty()
            || self.native_codec.version == 0
            || self.native_bytes.is_empty()
        {
            return Err(ContractError::InvalidData("incomplete native checkpoint envelope".into()));
        }
        let expected = match self.scope {
            CheckpointScope::World(id) => {
                if manifest.role != ContextRole::World { return Err(ContractError::InvalidData("world checkpoint role mismatch".into())); }
                id == manifest.key.world_instance_id
            }
            CheckpointScope::Entity(id) => {
                if manifest.role != ContextRole::Character && manifest.role != ContextRole::Item {
                    return Err(ContractError::InvalidData("entity checkpoint role mismatch".into()));
                }
                id.0 != 0
                    && manifest.owner_entity_id.is_none_or(|bound| bound == id)
            }
            CheckpointScope::Context(id) => {
                if manifest.role != ContextRole::Renderer {
                    return Err(ContractError::InvalidData("context checkpoint role mismatch".into()));
                }
                id == manifest.key.context_id
            }
        };
        if !expected {
            return Err(ContractError::StaleAuthority);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NativeContact {
    pub geometry: Geometry,
    /// Contact data is origin-defined but type/version qualified.
    pub material: Option<TypedValue>,
    pub solid: bool,
    pub owner_entity_id: Option<Id>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NativeGeometryBatch {
    pub frame_id: Id,
    pub revision: Revision,
    pub contacts: Vec<NativeContact>,
}

impl NativeGeometryBatch {
    /// Reject invalid/mismatched native contact geometry before an origin
    /// controller consumes it. This does NOT resolve collision for the source.
    pub fn validate_for(&self, request: &GeometryRequest) -> ContractResult<()> {
        fn valid_vec(v: crate::Vec3) -> bool {
            v.x.is_finite() && v.y.is_finite() && v.z.is_finite()
        }
        if request.frame_id.0 == 0 || self.frame_id != request.frame_id
            || !valid_vec(request.center)
            || !request.radius.is_finite() || request.radius <= 0.0 {
            return Err(ContractError::InvalidData("invalid native geometry request/frame".into()));
        }
        for contact in &self.contacts {
            if contact.owner_entity_id.is_some_and(|id| id.0 == 0) {
                return Err(ContractError::InvalidData("zero contact owner".into()));
            }
            if let Some(m) = &contact.material {
                if m.type_ref.namespace.is_empty() || m.type_ref.name.is_empty()
                    || m.type_ref.version == 0 {
                    return Err(ContractError::InvalidData("unversioned contact material".into()));
                }
            }
            let good = match &contact.geometry {
                Geometry::Aabb { frame_id, minimum, maximum } => {
                    *frame_id == request.frame_id && valid_vec(*minimum) && valid_vec(*maximum)
                        && minimum.x <= maximum.x && minimum.y <= maximum.y
                        && minimum.z <= maximum.z
                }
                Geometry::Plane { frame_id, normal, distance } => {
                    *frame_id == request.frame_id && valid_vec(*normal)
                        && distance.is_finite()
                        && (normal.x != 0.0 || normal.y != 0.0 || normal.z != 0.0)
                }
                Geometry::TriangleMesh { asset_id, transform } => {
                    asset_id.0 != 0 && transform.frame_id == request.frame_id
                        && valid_vec(transform.position) && valid_vec(transform.scale)
                        && transform.scale.x > 0.0 && transform.scale.y > 0.0
                        && transform.scale.z > 0.0
                        && transform.rotation_xyzw.iter().all(|v| v.is_finite())
                }
                Geometry::Custom(value) => {
                    !value.type_ref.namespace.is_empty() && !value.type_ref.name.is_empty()
                        && value.type_ref.version > 0
                }
            };
            if !good {
                return Err(ContractError::InvalidData("invalid native geometry contact".into()));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PixelFormat { Rgba8, Bgra8 }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlphaMode { Opaque, Straight, Premultiplied }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SurfaceSpace { Camera, World }

/// Actual source-produced pixels, NOT a resource name or composition plan.
/// Optional depth consists of per-pixel finite normalized depth samples.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NativeSurface {
    pub frame_number: u64,
    pub width: u32,
    pub height: u32,
    pub row_stride_bytes: u32,
    pub format: PixelFormat,
    pub alpha: AlphaMode,
    pub space: SurfaceSpace,
    pub z_order: i32,
    pub pixels: Vec<u8>,
    pub depth: Option<Vec<f32>>,
}

impl NativeSurface {
    pub fn validate(&self) -> ContractResult<()> {
        let minimum_row = self.width.checked_mul(4)
            .ok_or_else(|| ContractError::InvalidData("surface width overflow".into()))?;
        let expected = (self.row_stride_bytes as usize).checked_mul(self.height as usize)
            .ok_or_else(|| ContractError::InvalidData("surface length overflow".into()))?;
        if self.width == 0 || self.height == 0 || self.row_stride_bytes < minimum_row
            || self.pixels.len() != expected {
            return Err(ContractError::InvalidData("invalid native pixel buffer".into()));
        }
        if let Some(depth) = &self.depth {
            let count = (self.width as usize).checked_mul(self.height as usize)
                .ok_or_else(|| ContractError::InvalidData("depth length overflow".into()))?;
            if depth.len() != count || depth.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v)) {
                return Err(ContractError::InvalidData("invalid native depth buffer".into()));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NativeRenderPacket {
    pub context: ContextKey,
    pub frame: RenderFrame,
    pub camera: CameraState,
    pub surfaces: Vec<NativeSurface>,
}

impl NativeRenderPacket {
    pub fn validate(&self) -> ContractResult<()> {
        self.context.validate()?;
        if self.frame.width == 0 || self.frame.height == 0
            || self.frame.world_instance_id != self.context.world_instance_id
            || self.camera.owner_entity_id != self.frame.camera.owner_entity_id
            || self.camera.frame_id != self.frame.camera.frame_id
            || self.camera.view_column_major_4x4 != self.frame.camera.view_column_major_4x4
            || self.camera.projection_column_major_4x4 != self.frame.camera.projection_column_major_4x4
            || self.camera.data != self.frame.camera.data
            || self.camera.view_column_major_4x4.iter().any(|x| !x.is_finite())
            || self.camera.projection_column_major_4x4.iter().any(|x| !x.is_finite()) {
            return Err(ContractError::InvalidData("native render frame/camera mismatch".into()));
        }
        if self.surfaces.is_empty() {
            return Err(ContractError::InvalidData("no rendered native surfaces".into()));
        }
        for surface in &self.surfaces {
            if surface.frame_number != self.frame.frame_number { return Err(ContractError::InvalidData("stale native render surface".into())); }
            surface.validate()?;
        }
        Ok(())
    }
}

/// Receipt is an auditable outcome, never proof that a request was authorized
/// merely because an untrusted worker returned it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EffectReceipt {
    pub request_id: Id,
    pub receiver_context_id: Id,
    pub receiver_revision: Revision,
    pub committed_sequence: u64,
    pub result: InteractionResult,
}

impl EffectReceipt {
    pub fn validate(&self, request: &InteractionRequest, receiver_context_id: Id) -> ContractResult<()> {
        if self.request_id != request.id || self.result.request_id != request.id
            || self.receiver_context_id != receiver_context_id
            || self.committed_sequence == 0 {
            return Err(ContractError::InvalidData("invalid receiver effect receipt".into()));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthoritativeTick {
    pub clock: ClockStep,
    pub authority: AuthorityStamp,
}

impl AuthoritativeTick {
    pub fn validate(&self, expected_context: Id) -> ContractResult<()> {
        if self.authority.context_id != expected_context || self.authority.epoch == 0
            || self.authority.resource_key.is_empty() || self.clock.delta_nanos == 0 {
            return Err(ContractError::StaleAuthority);
        }
        Ok(())
    }
}

/// The object lives entirely in its source-native worker. No Send/Sync
/// obligation can force original scene internals into the host address space.
pub trait NativeContext {
    fn manifest(&self) -> &ContextManifest;
    fn initialize(&mut self, checkpoint: Option<&NativeCheckpoint>) -> ContractResult<()>;
    fn activate(&mut self) -> ContractResult<()>;
    fn suspend(&mut self) -> ContractResult<()>;
    fn checkpoint(&mut self, scope: CheckpointScope) -> ContractResult<NativeCheckpoint>;
    fn restore(&mut self, checkpoint: &NativeCheckpoint) -> ContractResult<()>;
    fn destroy(&mut self) -> ContractResult<()>;
}

/// Destination-owned original world ticks and original geometry.
pub trait WorldSimulation: NativeContext {
    fn step_world(&mut self, tick: &AuthoritativeTick, port: &mut dyn WorldPort) -> ContractResult<StepOutput>;
    fn geometry_native(&mut self, request: GeometryRequest) -> ContractResult<NativeGeometryBatch>;
}

/// A visiting character retains its source's original controller and solver.
pub trait CharacterController: NativeContext {
    fn attach_character(&mut self, entity: &EntityView, checkpoint: Option<&NativeCheckpoint>) -> ContractResult<NativeHandle>;
    fn step_character(&mut self, handle: NativeHandle, tick: &AuthoritativeTick, input: Option<&InputIntent>, destination: &mut dyn WorldPort) -> ContractResult<StepOutput>;
    fn detach_character(&mut self, handle: NativeHandle) -> ContractResult<()>;
}

/// An item's original module computes behavior, not authority or target writes.
pub trait ItemCapability: NativeContext {
    fn invoke_item(&mut self, item: &EntityView, action: &TypedValue, tick: &AuthoritativeTick, destination: &mut dyn WorldPort) -> ContractResult<Vec<InteractionRequest>>;
}

/// Rendering is source-owned; the generic compositor processes actual pixels.
pub trait NativeSceneRenderer: NativeContext {
    fn camera_native(&mut self, controller: Id, destination: &mut dyn WorldPort) -> ContractResult<CameraState>;
    fn render_native(&mut self, frame: &RenderFrame) -> ContractResult<NativeRenderPacket>;
}

/// Only the trusted receiver (through fenced platform commit) may apply
/// requests. A worker-supplied effect receipt must be checked against storage.
pub trait AuthorizedEffectReceiver: NativeContext {
    fn authorize_and_apply(&mut self, request: &InteractionRequest, port: &mut dyn WorldPort) -> ContractResult<EffectReceipt>;
}
