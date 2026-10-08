//! Independent native-module host. This crate knows no game names or assets.
//! Engine modules retain simulation and rendering logic and use WorldPort to
//! obtain external geometry and submit authoritative interactions.

pub mod render;
pub mod universe;
pub mod gateway;
pub mod scheduler;

use std::collections::{BTreeMap, BTreeSet};
use oasis_contracts::{
    AuthorityStamp, ClockStep, ContractError, ContractResult, EntityView,
    FrameMap, Geometry, GeometryRequest, GeometryResult, Id, InputIntent,
    InteractionReceiver, InteractionRequest,
    InteractionResult, InteractionTarget, NativeHandle, NativeModule,
    Revision, Snapshot, SpatialHit, SpatialQuery, StepOutput, WorldPort,
};

fn invalid(message: &str) -> ContractError {
    ContractError::InvalidData(message.into())
}

/// Transient world information and universal identity bindings. SQL is the
/// durable authority in deployments; this in-memory surface is one world shard.
#[derive(Default)]
pub struct WorldState {
    entities: BTreeMap<Id, EntityView>,
    geometry: BTreeMap<Id, Vec<Geometry>>,
    hits: BTreeMap<Id, Vec<SpatialHit>>,
    frames: BTreeMap<(Id, Id), FrameMap>,
    authorities: BTreeMap<String, AuthorityStamp>,
    pending: Vec<InteractionRequest>,
    submitted: BTreeSet<Id>,
}

impl WorldState {
    pub fn put_entity(&mut self, entity: EntityView) { self.entities.insert(entity.id, entity); }
    pub fn put_geometry(&mut self, frame: Id, shapes: Vec<Geometry>) {
        self.geometry.insert(frame, shapes);
    }
    pub fn put_hits(&mut self, frame: Id, hits: Vec<SpatialHit>) {
        self.hits.insert(frame, hits);
    }
    pub fn put_frame_map(&mut self, map: FrameMap) {
        self.frames.insert((map.source, map.destination), map);
    }
    /// A newer epoch fences previously issued permissions for this resource.
    pub fn grant_authority(&mut self, grant: AuthorityStamp) -> ContractResult<()> {
        if let Some(old) = self.authorities.get(&grant.resource_key) {
            if grant.epoch <= old.epoch {
                return Err(ContractError::StaleAuthority);
            }
        }
        self.authorities.insert(grant.resource_key.clone(), grant);
        Ok(())
    }
    pub fn entity(&self, id: Id) -> Option<&EntityView> { self.entities.get(&id) }
    pub fn pending_len(&self) -> usize { self.pending.len() }
    fn verify(&self, stamp: &AuthorityStamp) -> ContractResult<()> {
        match self.authorities.get(&stamp.resource_key) {
            Some(actual) if actual == stamp => Ok(()),
            _ => Err(ContractError::StaleAuthority),
        }
    }
}

/// Per-call view that exposes data, never native engine-specific objects.
pub struct Port<'a> { world: &'a mut WorldState }

impl<'a> Port<'a> {
    pub fn new(world: &'a mut WorldState) -> Self { Self { world } }
}

impl WorldPort for Port<'_> {
    fn geometry(&mut self, request: GeometryRequest) -> ContractResult<GeometryResult> {
        if !request.radius.is_finite() || request.radius < 0.0 {
            return Err(invalid("invalid geometry radius"));
        }
        Ok(GeometryResult {
            shapes: self.world.geometry.get(&request.frame_id).cloned().unwrap_or_default(),
            revision: Revision(0),
        })
    }
    fn query(&mut self, request: SpatialQuery) -> ContractResult<Vec<SpatialHit>> {
        Ok(self.world.hits.get(&request.frame_id).cloned().unwrap_or_default())
    }
    fn frame_map(&self, source: Id, destination: Id) -> ContractResult<FrameMap> {
        if source == destination {
            return Ok(FrameMap {
                source, destination,
                column_major_4x4: [
                    1.0,0.0,0.0,0.0, 0.0,1.0,0.0,0.0,
                    0.0,0.0,1.0,0.0, 0.0,0.0,0.0,1.0,
                ],
                source_units_per_destination_unit: 1.0,
            });
        }
        self.world.frames.get(&(source, destination)).cloned()
            .ok_or_else(|| invalid("no registered spatial transform"))
    }
    fn entity_view(&self, entity_id: Id) -> ContractResult<EntityView> {
        self.world.entities.get(&entity_id).cloned()
            .ok_or(ContractError::NotFound(entity_id))
    }
    fn submit_interaction(&mut self, request: InteractionRequest) -> ContractResult<()> {
        self.world.verify(&request.authority)?;
        if self.world.submitted.insert(request.id) {
            self.world.pending.push(request);
        }
        Ok(())
    }
}

/// Portable host: stores native modules by context, preserves native handles,
/// snapshots simulation state, and dispatches interactions without pair logic.
#[derive(Default)]
pub struct Host {
    world: WorldState,
    modules: BTreeMap<Id, Box<dyn NativeModule>>,
    handles: BTreeMap<Id, NativeHandle>,
    snapshots: BTreeMap<Id, Snapshot>,
    receivers: BTreeMap<Id, Box<dyn InteractionReceiver + Send>>,
    receipts: BTreeMap<Id, InteractionResult>,
}

impl Host {
    pub fn new() -> Self { Self::default() }
    pub fn world(&self) -> &WorldState { &self.world }
    pub fn world_mut(&mut self) -> &mut WorldState { &mut self.world }

    pub fn register_module(&mut self, context: Id, module: Box<dyn NativeModule>) -> ContractResult<()> {
        if self.modules.contains_key(&context) { return Err(invalid("execution context already registered")); }
        self.modules.insert(context, module);
        Ok(())
    }
    pub fn register_receiver(&mut self, entity: Id, receiver: Box<dyn InteractionReceiver + Send>) {
        self.receivers.insert(entity, receiver);
    }
    pub fn instantiate(
        &mut self, context: Id, entity: EntityView, initial: Option<&Snapshot>,
    ) -> ContractResult<NativeHandle> {
        if self.handles.contains_key(&entity.id) { return Err(invalid("entity already instantiated")); }
        let module = self.modules.get_mut(&context).ok_or(ContractError::NotFound(context))?;
        let handle = module.instantiate(&entity, initial)?;
        if handle.context_id != context { return Err(invalid("native handle belongs to another context")); }
        self.handles.insert(entity.id, handle);
        self.world.put_entity(entity);
        Ok(handle)
    }
    pub fn step(&mut self, context: Id, clock: ClockStep, inputs: &[InputIntent]) -> ContractResult<StepOutput> {
        let module = self.modules.get_mut(&context).ok_or(ContractError::NotFound(context))?;
        let mut port = Port::new(&mut self.world);
        let output = module.step(clock, inputs, &mut port)?;
        for snapshot in &output.state_changes {
            if !self.handles.contains_key(&snapshot.entity_id) {
                return Err(invalid("module returned unknown entity state"));
            }
            self.snapshots.insert(snapshot.entity_id, snapshot.clone());
        }
        for request in &output.interactions { port.submit_interaction(request.clone())?; }
        Ok(output)
    }
    pub fn snapshot(&mut self, entity: Id) -> ContractResult<Snapshot> {
        let handle = *self.handles.get(&entity).ok_or(ContractError::NotFound(entity))?;
        let module = self.modules.get(&handle.context_id)
            .ok_or(ContractError::NotFound(handle.context_id))?;
        let snapshot = module.snapshot(handle)?;
        if snapshot.entity_id != entity { return Err(invalid("snapshot identity mismatch")); }
        self.snapshots.insert(entity, snapshot.clone());
        Ok(snapshot)
    }
    pub fn restore(&mut self, entity: Id, state: &Snapshot) -> ContractResult<()> {
        if state.entity_id != entity { return Err(invalid("restore identity mismatch")); }
        let handle = *self.handles.get(&entity).ok_or(ContractError::NotFound(entity))?;
        let module = self.modules.get_mut(&handle.context_id)
            .ok_or(ContractError::NotFound(handle.context_id))?;
        module.restore(handle, state)?;
        self.snapshots.insert(entity, state.clone());
        Ok(())
    }
    pub fn remove(&mut self, entity: Id) -> ContractResult<()> {
        let handle = *self.handles.get(&entity).ok_or(ContractError::NotFound(entity))?;
        self.modules.get_mut(&handle.context_id)
            .ok_or(ContractError::NotFound(handle.context_id))?
            .remove(handle)?;
        self.handles.remove(&entity);
        self.snapshots.remove(&entity);
        Ok(())
    }
    /// Dispatch already-authorized requests to their target. When a receiver
    /// is absent the request fails explicitly; no game-dependent fallback.
    pub fn dispatch(&mut self) -> Vec<ContractResult<InteractionResult>> {
        let requests = std::mem::take(&mut self.world.pending);
        requests.into_iter().map(|request| {
            if let Some(previous) = self.receipts.get(&request.id) {
                return Ok(previous.clone());
            }
            self.world.verify(&request.authority)?;
            let target = match &request.target {
                InteractionTarget::Entity(id) | InteractionTarget::World(id) => *id,
                InteractionTarget::Region { .. } => return Err(invalid("region receiver not registered")),
            };
            let mut receiver = self.receivers.remove(&target)
                .ok_or(ContractError::NotFound(target))?;
            let result = receiver.apply_interaction(&request, &mut Port::new(&mut self.world));
            self.receivers.insert(target, receiver);
            let outcome = result?;
            if outcome.request_id != request.id { return Err(invalid("receiver returned wrong request id")); }
            self.receipts.insert(request.id, outcome.clone());
            Ok(outcome)
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis_contracts::{
        DefinitionRef, InteractionDisposition, ModuleDescriptor, TypeRef, TypedValue, Value,
    };

    struct Fixture {
        module: Id, context: Id, entities: BTreeMap<u64, Snapshot>,
        next: u64, expected_frame: Id,
    }
    impl Fixture {
        fn new(module: Id, context: Id, frame: Id) -> Self {
            Self { module, context, entities: BTreeMap::new(), next: 0, expected_frame: frame }
        }
    }
    impl NativeModule for Fixture {
        fn descriptor(&self) -> ModuleDescriptor {
            ModuleDescriptor {
                engine_id: self.module, module_id: self.module,
                contract_major: 0, contract_minor: 1,
                exported_interfaces: vec![], required_interfaces: vec![],
            }
        }
        fn instantiate(&mut self, entity: &EntityView, state: Option<&Snapshot>) -> ContractResult<NativeHandle> {
            self.next += 1;
            self.entities.insert(self.next, state.cloned().unwrap_or(Snapshot {
                entity_id: entity.id, state: entity.components.clone(),
                revision: Revision(0), binary_artifact: None,
            }));
            Ok(NativeHandle { context_id: self.context, native_slot: self.next })
        }
        fn step(&mut self, clock: ClockStep, _inputs: &[InputIntent], world: &mut dyn WorldPort) -> ContractResult<StepOutput> {
            let geometry = world.geometry(GeometryRequest {
                frame_id: self.expected_frame, center: oasis_contracts::Vec3{x:0.0,y:0.0,z:0.0},
                radius: 16.0, filter: None,
            })?;
            let mut state_changes = vec![];
            for snapshot in self.entities.values_mut() {
                snapshot.revision.0 += 1;
                snapshot.state.push(TypedValue {
                    type_ref: TypeRef {namespace:"fixture".into(),name:"tick".into(),version:1},
                    data: Value::UInt(clock.native_tick + geometry.shapes.len() as u64),
                });
                state_changes.push(snapshot.clone());
            }
            Ok(StepOutput { state_changes, interactions: vec![], emitted_events: vec![] })
        }
        fn snapshot(&self, handle: NativeHandle) -> ContractResult<Snapshot> {
            self.entities.get(&handle.native_slot).cloned().ok_or(ContractError::NotFound(self.context))
        }
        fn restore(&mut self, handle: NativeHandle, snapshot: &Snapshot) -> ContractResult<()> {
            let s = self.entities.get_mut(&handle.native_slot).ok_or(ContractError::NotFound(self.context))?;
            *s = snapshot.clone();
            Ok(())
        }
        fn remove(&mut self, handle: NativeHandle) -> ContractResult<()> {
            self.entities.remove(&handle.native_slot).ok_or(ContractError::NotFound(self.context))?;
            Ok(())
        }
    }
    fn entity(id: u128, module: u128) -> EntityView {
        EntityView {
            id: Id(id), definition: DefinitionRef { id: Id(module), version: 1 },
            revision: Revision(0), components: vec![], origin_module: Some(Id(module)),
        }
    }
    struct Receiver;
    impl InteractionReceiver for Receiver {
        fn apply_interaction(&mut self, request: &InteractionRequest, _world: &mut dyn WorldPort)
            -> ContractResult<InteractionResult> {
            Ok(InteractionResult {
                request_id: request.id,
                disposition: InteractionDisposition::Applied { result: None },
                effects: vec![],
            })
        }
    }
    #[test]
    fn two_independent_modules_use_identical_contract_without_knowing_each_other() {
        let mut host = Host::new();
        host.world_mut().put_geometry(Id(20), vec![Geometry::Aabb {
            frame_id: Id(20),
            minimum: oasis_contracts::Vec3{x:0.0,y:0.0,z:0.0},
            maximum: oasis_contracts::Vec3{x:10.0,y:10.0,z:0.0},
        }]);
        host.register_module(Id(100), Box::new(Fixture::new(Id(1), Id(100), Id(20)))).unwrap();
        host.register_module(Id(200), Box::new(Fixture::new(Id(2), Id(200), Id(30)))).unwrap();
        host.instantiate(Id(100), entity(500, 1), None).unwrap();
        host.instantiate(Id(200), entity(600, 2), None).unwrap();
        for context in [Id(100), Id(200)] {
            host.step(context, ClockStep{native_tick: 7, simulation_time_nanos: 100, delta_nanos:16}, &[]).unwrap();
        }
        let a = host.snapshot(Id(500)).unwrap();
        let b = host.snapshot(Id(600)).unwrap();
        assert_eq!(a.revision, Revision(1));
        assert_eq!(b.revision, Revision(1));
        assert_eq!(a.state[0].data, Value::UInt(8));
        assert_eq!(b.state[0].data, Value::UInt(7));
        host.remove(Id(500)).unwrap();
    }
    #[test]
    fn authority_fencing_prevents_stale_interaction_and_dispatch_is_generic() {
        let mut host = Host::new();
        let stamp = AuthorityStamp {resource_key:"entity-42".into(),context_id:Id(100),epoch:1};
        host.world_mut().grant_authority(stamp.clone()).unwrap();
        host.register_receiver(Id(42), Box::new(Receiver));
        let req = InteractionRequest {
            id: Id(99), source_entity_id: Id(5), target: InteractionTarget::Entity(Id(42)),
            operation: TypedValue{
                type_ref:TypeRef{namespace:"core".into(),name:"interaction".into(),version:1},
                data:Value::Null,
            }, authority: stamp.clone(), source_tick:1,
        };
        Port::new(host.world_mut()).submit_interaction(req.clone()).unwrap();
        Port::new(host.world_mut()).submit_interaction(req.clone()).unwrap();
        assert_eq!(host.world().pending_len(),1);
        let results=host.dispatch();
        assert!(matches!(&results[0], Ok(InteractionResult{disposition:InteractionDisposition::Applied{..},..})));
        let newer=AuthorityStamp{epoch:2,..stamp};
        host.world_mut().grant_authority(newer).unwrap();
        assert!(matches!(Port::new(host.world_mut()).submit_interaction(InteractionRequest{id:Id(1000),..req}),Err(ContractError::StaleAuthority)));
    }
    #[test]
    fn unknown_spatial_frame_fails_explicitly() {
        let mut world=WorldState::default();
        assert!(Port::new(&mut world).frame_map(Id(1),Id(2)).is_err());
    }
}
