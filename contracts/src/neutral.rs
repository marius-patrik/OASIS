//! Source-neutral contract vectors. These are structural fixtures, NOT proof
//! of original-game gameplay, replay fidelity, rendered scene semantics or MMO.
use crate::{
    AuthorityStamp, ClockStep, DefinitionRef, EntityView, Geometry, GeometryRequest, Id,
    InteractionRequest, InteractionTarget, Revision, Transform, TypeRef, TypedValue, Value, Vec3,
};
use crate::universal::{
    AlphaMode, AuthoritativeTick, CheckpointScope, ContextKey, ContextManifest, ContextRole,
    IsolationMode, NativeCheckpoint, NativeContact, NativeGeometryBatch, NativeSurface, PixelFormat,
    SurfaceSpace,
};

pub struct NeutralFixture {
    pub manifest: ContextManifest,
    pub tick: AuthoritativeTick,
    pub entity: EntityView,
    pub geometry_query: GeometryRequest,
    pub geometry_answer: NativeGeometryBatch,
    pub surface: NativeSurface,
    pub checkpoint: NativeCheckpoint,
    pub interaction: InteractionRequest,
}

fn fixture(dimensions: u8, seed: u128) -> NeutralFixture {
    let world = Id(seed * 10 + 1);
    let context = Id(seed * 10 + 2);
    let module = Id(seed * 10 + 3);
    let character = Id(seed * 10 + 4);
    let frame = Id(seed * 10 + 5);
    let key = ContextKey {world_instance_id: world, context_id: context, module_id: module};
    let digest = [dimensions; 32];
    let (num, den) = if dimensions == 2 { (60, 1) } else { (35, 1) };
    let manifest = ContextManifest {
        key, role: ContextRole::Character, owner_entity_id:Some(character), isolation: IsolationMode::Process,
        artifact_sha256: digest, ticks_per_second_num: num, ticks_per_second_den: den,
    };
    let native_codec = TypeRef {namespace:"neutral".into(), name:format!("native-{dimensions}d"), version:1};
    let entity = EntityView {
        id: character, definition: DefinitionRef{id:Id(seed*10+6), version:1},
        revision:Revision(1), components:vec![], origin_module:Some(module),
    };
    let geometry_query = GeometryRequest {
        frame_id:frame, center:Vec3{x:1.,y:2.,z:0.}, radius:3., filter:None,
    };
    let geometry_answer = NativeGeometryBatch {
        frame_id: frame, revision: Revision(2), contacts: vec![NativeContact {
            geometry: if dimensions == 2 {
                Geometry::Aabb{frame_id:frame,minimum:Vec3{x:0.,y:0.,z:0.},
                    maximum:Vec3{x:4.,y:4.,z:0.}}
            } else {
                Geometry::TriangleMesh {
                    asset_id:Id(seed*10+7), transform:Transform{
                        frame_id:frame,position:Vec3{x:0.,y:0.,z:0.},
                        rotation_xyzw:[0.,0.,0.,1.], scale:Vec3{x:1.,y:1.,z:1.},
                    }
                }
            },
            solid:true,
            material:Some(TypedValue {
                type_ref:TypeRef{namespace:"neutral".into(), name:"contact".into(),version:1},
                data:Value::String("generic-solid".into()),
            }),
            owner_entity_id:None,
        }],
    };
    let surface = NativeSurface {
        frame_number:7,width:2,height:2,row_stride_bytes:8,
        format:if dimensions == 2 {PixelFormat::Rgba8} else {PixelFormat::Bgra8},
        alpha:if dimensions == 2 {AlphaMode::Straight} else {AlphaMode::Opaque},
        space:if dimensions == 2 {SurfaceSpace::Camera} else {SurfaceSpace::World},
        z_order:0,
        pixels:vec![dimensions;16],
        depth:if dimensions == 2 {None} else {Some(vec![0.1,0.3,0.6,1.0])},
    };
    let checkpoint = NativeCheckpoint {
        context:key,scope:CheckpointScope::Entity(character),native_codec,
        artifact_sha256:digest,native_tick:7,state_revision:Revision(3),
        // A deterministic opaque test vector, NOT a lossless source-engine save.
        native_bytes:vec![dimensions, 0xa5, 0, 7],attached_components:vec![],
    };
    let tick = AuthoritativeTick {
        clock:ClockStep{native_tick:7,simulation_time_nanos:200_000_000,delta_nanos:16_666_667},
        authority:AuthorityStamp {
            resource_key:format!("world:{}",world.0),context_id:context,epoch:2,
        },
    };
    let interaction = InteractionRequest {
        id:Id(seed*10+8),source_entity_id:character,
        target:InteractionTarget::World(world),
        operation:TypedValue {
            type_ref:TypeRef{namespace:"neutral".into(),name:"touch".into(),version:1},
            data:Value::Bool(true),
        },
        authority:tick.authority.clone(),source_tick:7,
    };
    NeutralFixture{manifest,tick,entity,geometry_query,geometry_answer,surface,checkpoint,interaction}
}

/// Geometry confined to a plane, source-defined pixels, 60 Hz rational clock.
pub fn plane_2d() -> NeutralFixture { fixture(2, 11) }

/// Triangular 3D source geometry and depth pixels, 35 Hz rational clock.
pub fn volume_3d() -> NeutralFixture { fixture(3, 21) }
