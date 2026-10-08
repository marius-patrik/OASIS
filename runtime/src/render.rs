//! Native rendering composition: original render providers preserve ownership
//! of their pipelines and deliver generic geometry / surface contributions.
//! The platform assembles a plan; each graphics backend implements the GPU
//! resource import, depth composition, and final presentation separately.

use std::collections::BTreeMap;
use oasis_contracts::{
    CameraState, ContractError, ContractResult, Id, RenderContribution,
    RenderFrame, RenderProvider,
};

use crate::{Port, WorldState};

#[derive(Clone, Debug)]
pub struct ContributedFrame {
    pub source_id: Id,
    pub contribution: RenderContribution,
}

#[derive(Clone, Debug)]
pub struct CompositionPlan {
    pub world_instance_id: Id,
    pub frame_number: u64,
    pub camera: CameraState,
    /// The order is explicit and stable. Surface composition backends must
    /// use depth and camera metadata rather than assuming all layers are 2D.
    pub contributions: Vec<ContributedFrame>,
}

#[derive(Clone, Copy, Debug)]
pub struct FrameRequest {
    pub camera_provider: Id,
    pub controller_entity: Id,
    pub world_instance_id: Id,
    pub frame_number: u64,
    pub width: u32,
    pub height: u32,
    pub output_time_nanos: u128,
}

/// No world renderer or character renderer is intrinsically preferred; the
/// player controller selects which native camera owns the current view.
#[derive(Default)]
pub struct RenderPipeline {
    providers: BTreeMap<Id, Box<dyn RenderProvider>>,
    order: Vec<Id>,
}

impl RenderPipeline {
    pub fn new() -> Self { Self::default() }
    pub fn register(&mut self, id: Id, provider: Box<dyn RenderProvider>) -> ContractResult<()> {
        if self.providers.contains_key(&id) {
            return Err(ContractError::InvalidData("render source already registered".into()));
        }
        self.providers.insert(id, provider);
        self.order.push(id);
        Ok(())
    }
    pub fn remove(&mut self, id: Id) -> ContractResult<()> {
        self.providers.remove(&id).ok_or(ContractError::NotFound(id))?;
        self.order.retain(|source| *source != id);
        Ok(())
    }
    pub fn compose(&mut self, world: &mut WorldState, request: FrameRequest)
        -> ContractResult<CompositionPlan> {
        let FrameRequest {
            camera_provider, controller_entity, world_instance_id, frame_number,
            width, height, output_time_nanos,
        } = request;
        if width == 0 || height == 0 {
            return Err(ContractError::InvalidData("frame dimensions must be positive".into()));
        }
        let camera = self.providers.get_mut(&camera_provider)
            .ok_or(ContractError::NotFound(camera_provider))?
            .camera(controller_entity, &mut Port::new(world))?;
        if camera.owner_entity_id != controller_entity {
            return Err(ContractError::InvalidData("camera controller mismatch".into()));
        }
        let frame = RenderFrame {
            world_instance_id, frame_number, camera: camera.clone(),
            width, height, output_time_nanos,
        };
        let mut contributions = Vec::new();
        for source_id in &self.order {
            let provider = self.providers.get_mut(source_id)
                .ok_or(ContractError::NotFound(*source_id))?;
            for contribution in provider.contribute(&frame)? {
                contributions.push(ContributedFrame {
                    source_id: *source_id, contribution,
                });
            }
        }
        Ok(CompositionPlan { world_instance_id, frame_number, camera, contributions })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NativeRenderer {
        frame: Id,
        handle: &'static str,
    }
    impl RenderProvider for NativeRenderer {
        fn camera(
            &mut self, controller: Id, _world: &mut dyn oasis_contracts::WorldPort,
        ) -> ContractResult<CameraState> {
            Ok(CameraState {
                owner_entity_id: controller,
                frame_id: self.frame,
                view_column_major_4x4: [
                    1.,0.,0.,0., 0.,1.,0.,0., 0.,0.,1.,0., 0.,0.,0.,1.,
                ],
                projection_column_major_4x4: [
                    1.,0.,0.,0., 0.,1.,0.,0., 0.,0.,1.,0., 0.,0.,0.,1.,
                ],
                data: None,
            })
        }
        fn contribute(&mut self, frame: &RenderFrame)
            -> ContractResult<Vec<RenderContribution>> {
            Ok(vec![RenderContribution::SharedSurface {
                resource_handle: self.handle.into(),
                depth_handle: Some(format!("depth:{}", self.handle)),
                frame_number: frame.frame_number,
            }])
        }
    }

    #[test]
    fn preserves_native_surface_sources_and_origin_camera() {
        let mut pipe = RenderPipeline::new();
        pipe.register(Id(10), Box::new(NativeRenderer{frame:Id(30),handle:"world"})).unwrap();
        pipe.register(Id(20), Box::new(NativeRenderer{frame:Id(40),handle:"character"})).unwrap();
        let plan = pipe.compose(&mut WorldState::default(), FrameRequest {
            camera_provider: Id(20), controller_entity: Id(100),
            world_instance_id: Id(200), frame_number: 12,
            width: 1920, height: 1080, output_time_nanos: 1000,
        }).unwrap();
        assert_eq!(plan.camera.frame_id, Id(40));
        assert_eq!(plan.camera.owner_entity_id, Id(100));
        assert_eq!(plan.contributions.len(), 2);
        assert_eq!(plan.contributions[0].source_id, Id(10));
        assert_eq!(plan.contributions[1].source_id, Id(20));
        match &plan.contributions[1].contribution {
            RenderContribution::SharedSurface {resource_handle, depth_handle, frame_number} => {
                assert_eq!(resource_handle, "character");
                assert_eq!(depth_handle.as_deref(), Some("depth:character"));
                assert_eq!(*frame_number, 12);
            }
            _ => panic!("native surface was replaced by a proxy"),
        }
    }

    #[test]
    fn refuses_missing_camera_source_and_invalid_dimensions() {
        let mut pipe = RenderPipeline::new();
        let mut world = WorldState::default();
        assert!(matches!(pipe.compose(&mut world, FrameRequest{camera_provider:Id(1),controller_entity:Id(2),world_instance_id:Id(3),frame_number:1,width:10,height:10,output_time_nanos:0}),
                         Err(ContractError::NotFound(Id(1)))));
        pipe.register(Id(1), Box::new(NativeRenderer{frame:Id(4),handle:"world"})).unwrap();
        assert!(pipe.compose(&mut world, FrameRequest{camera_provider:Id(1),controller_entity:Id(2),world_instance_id:Id(3),frame_number:1,width:0,height:10,output_time_nanos:0}).is_err());
        pipe.remove(Id(1)).unwrap();
        assert!(pipe.compose(&mut world, FrameRequest{camera_provider:Id(1),controller_entity:Id(2),world_instance_id:Id(3),frame_number:1,width:10,height:10,output_time_nanos:0}).is_err());
    }
}
