//! Contract-envelope tests only. These DO NOT certify original engine logic.
//! Both original source lanes must reuse these inputs and add executable tests.
use oasis_contracts::{
    CameraState, ContractError, Id, InteractionDisposition, InteractionResult, RenderFrame,
    Revision,
};
use oasis_contracts::neutral::{plane_2d, volume_3d};
use oasis_contracts::universal::{
    CheckpointScope, ContextRole, EffectReceipt, LifecyclePhase, NativeRenderPacket, NativeSurface,
};

fn identity() -> [f64; 16] {
    [1.,0.,0.,0., 0.,1.,0.,0., 0.,0.,1.,0., 0.,0.,0.,1.]
}

#[test]
fn independent_source_neutral_2d_and_3d_vectors_validate() {
    for sample in [plane_2d(), volume_3d()] {
        sample.manifest.validate().unwrap();
        sample.tick.validate(sample.manifest.key.context_id).unwrap();
        sample.checkpoint.validate_for(&sample.manifest).unwrap();
        sample.surface.validate().unwrap();
        sample.geometry_answer.validate_for(&sample.geometry_query).unwrap();
        assert_eq!(sample.geometry_query.frame_id, sample.geometry_answer.frame_id);
        assert_eq!(sample.checkpoint.native_tick, sample.tick.clock.native_tick);
        assert_eq!(sample.interaction.source_entity_id, sample.entity.id);
    }
}

#[test]
fn independent_contexts_and_native_tick_rates_do_not_alias() {
    let a=plane_2d();
    let b=volume_3d();
    assert_ne!(a.manifest.key, b.manifest.key);
    assert_ne!(a.manifest.artifact_sha256, b.manifest.artifact_sha256);
    assert_ne!(a.manifest.ticks_per_second_num, b.manifest.ticks_per_second_num);
    assert!(a.tick.validate(b.manifest.key.context_id).is_err());
    assert!(a.checkpoint.validate_for(&b.manifest).is_err());
}

#[test]
fn geometry_frames_and_contacts_must_be_native_well_formed() {
    let mut sample = plane_2d();
    sample.geometry_answer.validate_for(&sample.geometry_query).unwrap();
    sample.geometry_answer.frame_id = Id(999);
    assert!(sample.geometry_answer.validate_for(&sample.geometry_query).is_err());
    sample.geometry_answer.frame_id = sample.geometry_query.frame_id;
    sample.geometry_query.radius = f64::NAN;
    assert!(sample.geometry_answer.validate_for(&sample.geometry_query).is_err());
    sample.geometry_query.radius = 3.0;
    sample.geometry_answer.contacts[0].owner_entity_id = Some(Id(0));
    assert!(sample.geometry_answer.validate_for(&sample.geometry_query).is_err());
}

#[test]
fn opaque_state_roundtrips_without_json_float_coercion() {
    let sample=plane_2d();
    let copy=sample.checkpoint.clone();
    assert_eq!(copy.native_bytes, sample.checkpoint.native_bytes);
    assert_eq!(copy.native_tick,7);
    assert_eq!(copy.state_revision,Revision(3));
    // Byte equality proves transport integrity ONLY, not complete native save.
}

#[test]
fn checkpoint_envelope_rejects_forged_context_digest_and_incomplete_payload() {
    let fixture=volume_3d();
    let mut saved=fixture.checkpoint.clone();
    saved.context.context_id=Id(9999);
    assert!(matches!(saved.validate_for(&fixture.manifest),Err(ContractError::StaleAuthority)));
    saved=fixture.checkpoint.clone();
    saved.artifact_sha256=[9;32];
    assert!(saved.validate_for(&fixture.manifest).is_err());
    saved=fixture.checkpoint.clone();
    saved.scope=CheckpointScope::Entity(Id(8888));
    assert!(saved.validate_for(&fixture.manifest).is_err());
    saved=fixture.checkpoint.clone();
    saved.native_bytes.clear();
    assert!(saved.validate_for(&fixture.manifest).is_err());
    let mut multi=fixture.manifest.clone();
    multi.owner_entity_id=None;
    multi.validate().unwrap();
    let mut peer=fixture.checkpoint.clone();
    peer.scope=CheckpointScope::Entity(Id(777));
    peer.validate_for(&multi).unwrap();
    // This is envelope-only: host must verify peer ID ↔ native slot ownership.
    saved=fixture.checkpoint.clone();
    saved.scope=CheckpointScope::World(fixture.manifest.key.world_instance_id);
    assert!(saved.validate_for(&fixture.manifest).is_err());
}

#[test]
fn native_lifecycle_rejects_illegal_rewind_and_running_snapshots() {
    use LifecyclePhase::{Created,Destroyed,Initialized,Running,Suspended};
    assert_eq!(Created.transition(Initialized).unwrap(),Initialized);
    assert_eq!(Initialized.transition(Running).unwrap(),Running);
    assert_eq!(Running.transition(Suspended).unwrap(),Suspended);
    assert!(Suspended.can_checkpoint());
    assert!(!Running.can_checkpoint());
    assert!(Running.transition(Created).is_err());
    assert!(Running.transition(Destroyed).is_err());
    assert!(Destroyed.transition(Running).is_err());
    assert_eq!(Suspended.transition(Destroyed).unwrap(),Destroyed);
}

#[test]
fn native_pixels_must_be_real_well_formed_bytes_not_resource_handles() {
    let mut surface=plane_2d().surface;
    surface.validate().unwrap();
    surface.pixels.pop();
    assert!(surface.validate().is_err());
    let mut surface=volume_3d().surface;
    surface.depth=Some(vec![0.1, f32::NAN, 0.4, 1.0]);
    assert!(surface.validate().is_err());
    let mut surface=volume_3d().surface;
    surface.depth=Some(vec![0.5]);
    assert!(surface.validate().is_err());
    let mut surface=plane_2d().surface;
    surface.row_stride_bytes=0;
    assert!(surface.validate().is_err());
    let mut surface=plane_2d().surface;
    surface.width=u32::MAX;
    assert!(surface.validate().is_err());
}

fn packet(surface:NativeSurface) -> NativeRenderPacket {
    let f=plane_2d();
    let camera=CameraState {
        owner_entity_id:f.entity.id,
        frame_id:f.geometry_query.frame_id,
        view_column_major_4x4:identity(),
        projection_column_major_4x4:identity(),
        data:None,
    };
    NativeRenderPacket {
        context:f.manifest.key,
        frame:RenderFrame {
            world_instance_id:f.manifest.key.world_instance_id,
            frame_number:surface.frame_number,
            camera:camera.clone(),width:surface.width,height:surface.height,
            output_time_nanos:100_000,
        },
        camera,surfaces:vec![surface],
    }
}

#[test]
fn actual_native_pixel_packets_check_source_world_camera_and_frame() {
    let surface=plane_2d().surface;
    let mut output=packet(surface);
    output.validate().unwrap();
    output.surfaces[0].frame_number+=1;
    assert!(output.validate().is_err());
    output.surfaces[0].frame_number=output.frame.frame_number;
    output.frame.world_instance_id=Id(99999);
    assert!(output.validate().is_err());
    output.frame.world_instance_id=output.context.world_instance_id;
    output.surfaces.clear();
    assert!(output.validate().is_err());
}

#[test]
fn effect_receipts_do_not_self_authorize_or_cross_receiver() {
    let f=volume_3d();
    let receipt=EffectReceipt {
        request_id:f.interaction.id,
        receiver_context_id:Id(500),
        receiver_revision:Revision(4),
        committed_sequence:9,
        result:InteractionResult {
            request_id:f.interaction.id,
            disposition:InteractionDisposition::Applied{result:None},
            effects:vec![],
        },
    };
    receipt.validate(&f.interaction,Id(500)).unwrap();
    assert!(receipt.validate(&f.interaction,Id(501)).is_err());
    let mut stale=receipt;
    stale.committed_sequence=0;
    assert!(stale.validate(&f.interaction,Id(500)).is_err());
}

#[test]
fn role_and_worker_identity_must_be_explicit() {
    let mut source=plane_2d().manifest;
    source.validate().unwrap();
    source.artifact_sha256=[0;32];
    assert!(source.validate().is_err());
    source.artifact_sha256=[1;32];
    source.ticks_per_second_den=0;
    assert!(source.validate().is_err());
    source.ticks_per_second_den=1;
    source.key.context_id=Id(0);
    assert!(source.validate().is_err());
    source.key.context_id=Id(112);
    source.role=ContextRole::World;
    assert!(source.validate().is_err());
    source.owner_entity_id=None;
    assert!(source.validate().is_ok());
    // The test never treats Process isolation as a security sandbox.
}
