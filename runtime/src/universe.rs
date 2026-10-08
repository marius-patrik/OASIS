//! Engine-independent MMO world/session coordination.
//!
//! This is the authoritative *in-process* simulation coordinator. Its
//! authenticator must be backed by a trusted identity provider; clients never
//! choose their own user, player, or character IDs. Durable transactions,
//! distributed authority and transport belong to the server/store integrations.

use std::collections::{BTreeMap, BTreeSet};
use std::time::SystemTime;

use oasis_contracts::{
    ClockStep, ContractError, ContractResult, EntityView, Id, InputIntent,
    NativeModule, Snapshot, StepOutput, TypeRef, TypedValue, Value,
};

use crate::Host;

fn invalid(message: &str) -> ContractError {
    ContractError::InvalidData(message.into())
}

/// Issued by a trusted authentication backend, never deserialized from a
/// client-supplied identity claim.
#[derive(Clone, Debug)]
pub struct Principal {
    pub user_id: Id,
    pub player_id: Id,
    pub character_id: Id,
    pub expires_at: SystemTime,
}

pub trait SessionAuthority: Send + Sync {
    fn authenticate(&self, credential: &str) -> ContractResult<Principal>;
}

/// A handle local to one authenticated connection; changing the generation
/// immediately invalidates the prior connection for this character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionTicket {
    pub connection_id: u64,
    pub character_id: Id,
}

/// Durable authority resolved by a trusted storage provider. Clients never
/// choose either the hosted world or the execution context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PersistedPlacement {
    pub world_instance_id: Id,
    pub authority_context_id: Id,
    /// Monotonic fencing epoch from the same committed database lease.
    pub authority_epoch: u64,
}

/// A claim attached to native game state before the scheduler can advance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimulationLease {
    pub entity_id: Id,
    pub context_id: Id,
    pub epoch: u64,
}

struct WorldShard {
    host: Host,
    contexts: BTreeMap<Id, Id>,
    pending: Vec<InputIntent>,
    tick: u64,
}

impl Default for WorldShard {
    fn default() -> Self {
        Self { host: Host::new(), contexts: BTreeMap::new(), pending: Vec::new(), tick: 0 }
    }
}

/// Simultaneous world shards, portable entities, identity sessions, and
/// native-module dispatch. No game name or partner-specific behavior occurs.
#[derive(Default)]
pub struct Universe {
    worlds: BTreeMap<Id, WorldShard>,
    entities: BTreeMap<Id, EntityView>,
    presence: BTreeMap<Id, Id>,
    authority_epochs: BTreeMap<Id, u64>,
    connections: BTreeMap<u64, Principal>,
    active_connection: BTreeMap<Id, u64>,
    next_connection: u64,
}

impl Universe {
    pub fn new() -> Self { Self::default() }

    pub fn add_world(&mut self, world: Id) -> ContractResult<()> {
        if self.worlds.contains_key(&world) { return Err(invalid("world already exists")); }
        self.worlds.insert(world, WorldShard::default());
        Ok(())
    }

    /// Each shard may host many native modules. A carried character continues
    /// executing its *origin* module when present in a different world.
    pub fn add_module(
        &mut self, world: Id, context: Id, module: Box<dyn NativeModule>,
    ) -> ContractResult<()> {
        let descriptor = module.descriptor();
        if descriptor.contract_major != 0 || descriptor.contract_minor < 1 {
            return Err(invalid("incompatible native module ABI"));
        }
        let shard = self.worlds.get_mut(&world).ok_or(ContractError::NotFound(world))?;
        if shard.contexts.contains_key(&descriptor.module_id) {
            return Err(invalid("module already registered in world"));
        }
        shard.host.register_module(context, module)?;
        shard.contexts.insert(descriptor.module_id, context);
        Ok(())
    }

    /// The caller is a trusted database/adapter loader, not a network user.
    pub fn register_entity(&mut self, entity: EntityView) -> ContractResult<()> {
        if entity.origin_module.is_none() { return Err(invalid("origin module required")); }
        if self.entities.contains_key(&entity.id) { return Err(invalid("entity already registered")); }
        self.entities.insert(entity.id, entity);
        Ok(())
    }

    /// Cold-start restoration from an authoritative durable store. The
    /// caller must verify the database authority lease and session before
    /// passing a placement. No client input reaches this trusted API.
    ///
    /// The original native module remains the owner of entity logic, even
    /// when the destination uses another world renderer or physics module.
    pub fn recover_entity(
        &mut self,
        entity: EntityView,
        snapshot: Option<&Snapshot>,
        placement: Option<PersistedPlacement>,
    ) -> ContractResult<()> {
        if self.entities.contains_key(&entity.id) {
            return Err(invalid("cannot restore already-loaded entity"));
        }
        let origin = entity.origin_module.ok_or_else(||invalid("origin module missing"))?;
        if snapshot.is_some_and(|s|s.entity_id != entity.id) {
            return Err(invalid("snapshot does not belong to entity"));
        }
        if let Some(placement) = placement {
            if placement.authority_epoch == 0 {
                return Err(ContractError::StaleAuthority);
            }
            let shard = self.worlds.get_mut(&placement.world_instance_id)
                .ok_or(ContractError::NotFound(placement.world_instance_id))?;
            let context = *shard.contexts.get(&origin)
                .ok_or_else(||invalid("world has no native origin module"))?;
            if context != placement.authority_context_id {
                return Err(ContractError::StaleAuthority);
            }
            // Instantiation happens before either platform identity or
            // presence becomes visible to network clients.
            shard.host.instantiate(context,entity.clone(),snapshot)?;
            self.presence.insert(entity.id,placement.world_instance_id);
            self.authority_epochs.insert(entity.id,placement.authority_epoch);
        }
        self.entities.insert(entity.id,entity);
        Ok(())
    }

    pub fn connect(
        &mut self, auth: &dyn SessionAuthority, credential: &str,
    ) -> ContractResult<SessionTicket> {
        let principal = auth.authenticate(credential)?;
        if principal.expires_at <= SystemTime::now() {
            return Err(invalid("session expired"));
        }
        if !self.entities.contains_key(&principal.character_id) {
            return Err(ContractError::NotFound(principal.character_id));
        }
        self.next_connection = self.next_connection.checked_add(1)
            .ok_or_else(|| invalid("session counter exhausted"))?;
        let ticket = SessionTicket {
            connection_id: self.next_connection,
            character_id: principal.character_id,
        };
        self.connections.insert(ticket.connection_id, principal);
        if let Some(previous) = self.active_connection.insert(ticket.character_id, ticket.connection_id) {
            self.connections.remove(&previous);
        }
        Ok(ticket)
    }

    pub fn disconnect(&mut self, ticket: SessionTicket) {
        self.connections.remove(&ticket.connection_id);
        if self.active_connection.get(&ticket.character_id) == Some(&ticket.connection_id) {
            self.active_connection.remove(&ticket.character_id);
        }
        // Presence persists across network reconnect; not an entity deletion.
    }

    fn verify(&self, ticket: SessionTicket) -> ContractResult<&Principal> {
        if self.active_connection.get(&ticket.character_id) != Some(&ticket.connection_id) {
            return Err(invalid("stale connection"));
        }
        let principal = self.connections.get(&ticket.connection_id)
            .ok_or_else(|| invalid("invalid connection"))?;
        if principal.character_id != ticket.character_id ||
            principal.expires_at <= SystemTime::now() {
            return Err(invalid("session expired or mismatched"));
        }
        Ok(principal)
    }

    pub fn presence(&self, character: Id) -> Option<Id> {
        self.presence.get(&character).copied()
    }

    /// Preflight target module, snapshot origin state, detach origin and
    /// instantiate in destination. A failed target instantiation reattaches
    /// the origin; multi-process durable handoff requires the SQL store.
    pub fn enter_world(&mut self, ticket: SessionTicket, destination: Id)
        -> ContractResult<()> {
        self.verify(ticket)?;
        let entity = self.entities.get(&ticket.character_id)
            .ok_or(ContractError::NotFound(ticket.character_id))?.clone();
        let origin = entity.origin_module.ok_or_else(|| invalid("origin module missing"))?;
        let target_context = *self.worlds.get(&destination)
            .ok_or(ContractError::NotFound(destination))?
            .contexts.get(&origin)
            .ok_or_else(|| invalid("destination lacks native origin module"))?;
        let source = self.presence.get(&entity.id).copied();
        if source == Some(destination) { return Ok(()); }

        let state = match source {
            Some(old) => Some(self.worlds.get_mut(&old)
                .ok_or(ContractError::NotFound(old))?
                .host.snapshot(entity.id)?),
            None => None,
        };
        if let Some(old) = source {
            self.worlds.get_mut(&old).ok_or(ContractError::NotFound(old))?
                .host.remove(entity.id)?;
        }
        let result = self.worlds.get_mut(&destination)
            .ok_or(ContractError::NotFound(destination))?
            .host.instantiate(target_context, entity.clone(), state.as_ref());
        if let Err(failure) = result {
            if let Some(old) = source {
                let old_context = *self.worlds.get(&old)
                    .and_then(|shard| shard.contexts.get(&origin))
                    .ok_or_else(|| invalid("rollback context disappeared"))?;
                self.worlds.get_mut(&old).ok_or(ContractError::NotFound(old))?
                    .host.instantiate(old_context, entity, state.as_ref())?;
            }
            return Err(failure);
        }
        self.presence.insert(ticket.character_id, destination);
        Ok(())
    }

    /// Caller may submit input only for the character authenticated to this
    /// connection. The client never supplies a controller entity identifier.
    pub fn input(&mut self, ticket: SessionTicket, value: TypedValue)
        -> ContractResult<()> {
        self.verify(ticket)?;
        let world = self.presence.get(&ticket.character_id).copied()
            .ok_or_else(|| invalid("character is not present in any world"))?;
        let shard = self.worlds.get_mut(&world).ok_or(ContractError::NotFound(world))?;
        if shard.pending.len() >= 4096 {
            return Err(invalid("input queue full"));
        }
        shard.pending.push(InputIntent {
            controller_entity_id: ticket.character_id,
            intent: value,
        });
        Ok(())
    }

    /// Independently scheduled by the authoritative world loop. Native
    /// modules receive only input for entities whose source module they own.
    pub fn step_world(&mut self, world: Id, delta_nanos: u64)
        -> ContractResult<Vec<StepOutput>> {
        if delta_nanos == 0 { return Err(invalid("simulation delta must be positive")); }
        let shard = self.worlds.get_mut(&world).ok_or(ContractError::NotFound(world))?;
        let next_tick = shard.tick.checked_add(1)
            .ok_or_else(|| invalid("simulation tick overflow"))?;
        let mut outputs = Vec::new();
        let mut consumed = BTreeSet::new();
        for (module_id, context) in &shard.contexts {
            let inputs: Vec<_> = shard.pending.iter().filter(|input| {
                self.entities.get(&input.controller_entity_id)
                    .and_then(|e| e.origin_module) == Some(*module_id)
            }).cloned().collect();
            let clock = ClockStep {
                native_tick: next_tick,
                simulation_time_nanos: u128::from(next_tick) * u128::from(delta_nanos),
                delta_nanos,
            };
            let output = shard.host.step(*context, clock, &inputs)?;
            for input in &inputs { consumed.insert(input.controller_entity_id); }
            outputs.push(output);
        }
        shard.pending.retain(|intent| !consumed.contains(&intent.controller_entity_id));
        shard.tick = next_tick;
        Ok(outputs)
    }

    /// Interest set v0: all characters in one world, with snapshots retained
    /// by original native engine logic. Spatial filtering belongs to the
    /// scalable replication implementation.
    pub fn observe(&mut self, ticket: SessionTicket) -> ContractResult<Vec<Snapshot>> {
        self.verify(ticket)?;
        let world = self.presence.get(&ticket.character_id).copied()
            .ok_or_else(|| invalid("character has no world presence"))?;
        let ids: Vec<Id> = self.presence.iter()
            .filter_map(|(id, world_id)| (*world_id == world).then_some(*id))
            .collect();
        let shard = self.worlds.get_mut(&world).ok_or(ContractError::NotFound(world))?;
        ids.into_iter().map(|id| shard.host.snapshot(id)).collect()
    }

    /// Native state cannot be advanced by an authority-enforcing scheduler
    /// unless *every* participating entity has its own valid fencing epoch.
    /// Validate all claims before invoking any native module's world step.
    pub fn simulation_leases(&self, world: Id) -> ContractResult<Vec<SimulationLease>> {
        let shard = self.worlds.get(&world).ok_or(ContractError::NotFound(world))?;
        self.presence.iter().filter(|(_,w)|**w==world).map(|(entity,_)| {
            let origin = self.entities.get(entity).and_then(|e|e.origin_module)
                .ok_or(ContractError::NotFound(*entity))?;
            let context_id = *shard.contexts.get(&origin)
                .ok_or(ContractError::StaleAuthority)?;
            let epoch = *self.authority_epochs.get(entity)
                .ok_or(ContractError::StaleAuthority)?;
            Ok(SimulationLease{entity_id:*entity,context_id,epoch})
        }).collect()
    }

    pub fn has_entity(&self, id: Id) -> bool { self.entities.contains_key(&id) }
    pub fn context_for(&self, world: Id, origin_module: Id) -> Option<Id> {
        self.worlds.get(&world)
            .and_then(|shard|shard.contexts.get(&origin_module).copied())
    }
    pub fn has_world(&self, id: Id) -> bool { self.worlds.contains_key(&id) }
    pub fn world_count(&self) -> usize { self.worlds.len() }
    pub fn connected_count(&self) -> usize { self.active_connection.len() }
    pub fn entity_count(&self) -> usize { self.entities.len() }
}

/// A convenience for uniform player intent payloads in the first network
/// transport. Adapter-specific control semantics remain typed and extensible.
pub fn text_intent(action: impl Into<String>) -> TypedValue {
    TypedValue {
        type_ref: TypeRef {
            namespace: "oasis".into(), name: "input.text".into(), version: 1,
        },
        data: Value::String(action.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::time::Duration;
    use oasis_contracts::{
        DefinitionRef, ModuleDescriptor, NativeHandle, Revision, WorldPort,
    };

    struct TestAuth(BTreeMap<String, Principal>);
    impl SessionAuthority for TestAuth {
        fn authenticate(&self, credential: &str) -> ContractResult<Principal> {
            self.0.get(credential).cloned().ok_or_else(|| invalid("unauthorized"))
        }
    }

    struct NativeFixture {
        id: Id, context: Id, native: BTreeMap<u64, Snapshot>, next: u64,
        ticks: u64,
    }
    impl NativeFixture {
        fn new(id: u128, context: u128) -> Self {
            Self { id: Id(id), context: Id(context),
                   native: BTreeMap::new(), next: 0, ticks: 0 }
        }
    }
    impl NativeModule for NativeFixture {
        fn descriptor(&self) -> ModuleDescriptor {
            ModuleDescriptor {
                engine_id: self.id, module_id: self.id,
                contract_major: 0, contract_minor: 1,
                exported_interfaces: vec![], required_interfaces: vec![],
            }
        }
        fn instantiate(&mut self, entity: &EntityView, snapshot: Option<&Snapshot>)
            -> ContractResult<NativeHandle> {
            self.next += 1;
            self.native.insert(self.next, snapshot.cloned().unwrap_or(Snapshot {
                entity_id: entity.id, state: entity.components.clone(),
                revision: Revision(0), binary_artifact: None,
            }));
            Ok(NativeHandle { context_id:self.context,native_slot:self.next })
        }
        fn step(&mut self, _clock: ClockStep, inputs: &[InputIntent],
                _world: &mut dyn WorldPort) -> ContractResult<StepOutput> {
            self.ticks += 1;
            let mut state_changes = Vec::new();
            for s in self.native.values_mut() {
                if inputs.iter().any(|input| input.controller_entity_id == s.entity_id) {
                    s.revision.0 += 1;
                }
                state_changes.push(s.clone());
            }
            Ok(StepOutput {
                state_changes, interactions: vec![], emitted_events: vec![],
            })
        }
        fn snapshot(&self, handle: NativeHandle) -> ContractResult<Snapshot> {
            self.native.get(&handle.native_slot).cloned()
                .ok_or(ContractError::NotFound(self.context))
        }
        fn restore(&mut self, handle: NativeHandle, snapshot: &Snapshot)
            -> ContractResult<()> {
            *self.native.get_mut(&handle.native_slot)
                .ok_or(ContractError::NotFound(self.context))? = snapshot.clone();
            Ok(())
        }
        fn remove(&mut self, handle: NativeHandle) -> ContractResult<()> {
            self.native.remove(&handle.native_slot)
                .ok_or(ContractError::NotFound(self.context))?;
            Ok(())
        }
    }

    fn entity(id: u128, origin: u128) -> EntityView {
        EntityView {
            id: Id(id), definition: DefinitionRef { id:Id(origin),version:1 },
            revision: Revision(0), components:vec![], origin_module:Some(Id(origin)),
        }
    }
    fn setup() -> (Universe, TestAuth) {
        let mut server = Universe::new();
        server.add_world(Id(10)).unwrap();
        server.add_world(Id(20)).unwrap();
        // Same source runtime registered independently in both worlds.
        server.add_module(Id(10),Id(101),Box::new(NativeFixture::new(1,101))).unwrap();
        server.add_module(Id(20),Id(201),Box::new(NativeFixture::new(1,201))).unwrap();
        // Another native game is *only* present in world 20.
        server.add_module(Id(20),Id(202),Box::new(NativeFixture::new(2,202))).unwrap();
        server.register_entity(entity(501,1)).unwrap();
        server.register_entity(entity(502,2)).unwrap();
        let expiry = SystemTime::now() + Duration::from_secs(3600);
        let auth = TestAuth(BTreeMap::from([
            ("first".into(),Principal{
                user_id:Id(601),player_id:Id(701),
                character_id:Id(501),expires_at:expiry,
            }),
            ("second".into(),Principal{
                user_id:Id(602),player_id:Id(702),
                character_id:Id(502),expires_at:expiry,
            }),
        ]));
        (server,auth)
    }

    #[test]
    fn independent_worlds_preserve_native_character_and_revision() {
        let (mut server,auth) = setup();
        let a = server.connect(&auth,"first").unwrap();
        let b = server.connect(&auth,"second").unwrap();
        server.enter_world(a,Id(10)).unwrap();
        // Requires origin module in destination. Never substitutes a host
        // world's character controller.
        assert!(server.enter_world(b,Id(10)).is_err());
        server.enter_world(b,Id(20)).unwrap();
        server.input(a,text_intent("move")).unwrap();
        server.step_world(Id(10),16_666_667).unwrap();
        let before = server.observe(a).unwrap();
        assert_eq!(before[0].entity_id,Id(501));
        assert_eq!(before[0].revision,Revision(1));
        server.enter_world(a,Id(20)).unwrap();
        assert_eq!(server.presence(Id(501)),Some(Id(20)));
        assert_eq!(server.observe(b).unwrap().len(),2);
        let after = server.observe(a).unwrap();
        assert_eq!(after.iter().find(|s|s.entity_id==Id(501)).unwrap().revision,
                   Revision(1));
        assert_eq!(server.entity_count(),2);
    }

    #[test]
    fn reconnect_fences_old_connection_and_does_not_delete_character() {
        let (mut server,auth) = setup();
        let original = server.connect(&auth,"first").unwrap();
        server.enter_world(original,Id(10)).unwrap();
        let replacement = server.connect(&auth,"first").unwrap();
        assert!(server.input(original,text_intent("spoof")).is_err());
        server.input(replacement,text_intent("valid")).unwrap();
        server.disconnect(original);
        assert_eq!(server.connected_count(),1);
        assert_eq!(server.presence(Id(501)),Some(Id(10)));
        assert_eq!(server.observe(replacement).unwrap().len(),1);
    }

    #[test]
    fn untrusted_and_expired_credentials_never_create_sessions() {
        let (mut server,mut auth) = setup();
        assert!(server.connect(&auth,"unknown").is_err());
        assert_eq!(server.connected_count(),0);
        auth.0.get_mut("first").unwrap().expires_at=SystemTime::UNIX_EPOCH;
        assert!(server.connect(&auth,"first").is_err());
    }

    #[test]
    fn incompatible_source_engine_refuses_travel_without_duplicating_character() {
        let (mut server,auth) = setup();
        let b=server.connect(&auth,"second").unwrap();
        server.enter_world(b,Id(20)).unwrap();
        assert!(server.enter_world(b,Id(10)).is_err());
        assert_eq!(server.presence(Id(502)),Some(Id(20)));
        assert_eq!(server.observe(b).unwrap().len(),1);
    }
    #[test]
    fn two_network_clients_share_world_without_identity_spoofing() {
        use crate::gateway::LoopbackGateway;
        use std::io::{BufRead, BufReader, Write};
        use std::net::TcpStream;
        use std::sync::{Arc, Mutex};

        fn send(socket: &mut TcpStream, reader: &mut BufReader<TcpStream>,
                command: &str) -> String {
            writeln!(socket,"{command}").unwrap();
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            line.trim().to_owned()
        }

        let (universe,auth)=setup();
        let shared=Arc::new(Mutex::new(universe));
        let gateway=LoopbackGateway::bind(Arc::clone(&shared),Arc::new(auth)).unwrap();
        let mut a=TcpStream::connect(gateway.local_addr()).unwrap();
        let mut b=TcpStream::connect(gateway.local_addr()).unwrap();
        a.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        b.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let mut ar=BufReader::new(a.try_clone().unwrap());
        let mut br=BufReader::new(b.try_clone().unwrap());

        assert_eq!(send(&mut a,&mut ar,"INPUT unauthorized"),"ERR auth-required");
        assert_eq!(send(&mut a,&mut ar,"AUTH first"),"OK auth");
        assert_eq!(send(&mut b,&mut br,"AUTH second"),"OK auth");
        assert_eq!(send(&mut a,&mut ar,"JOIN 20"),"OK joined");
        assert_eq!(send(&mut b,&mut br,"JOIN 20"),"OK joined");
        assert_eq!(send(&mut a,&mut ar,"INPUT walk"),"OK queued");
        shared.lock().unwrap().step_world(Id(20),16_666_667).unwrap();

        assert_eq!(send(&mut b,&mut br,"POLL"),"COUNT 2");
        let mut item1=String::new();
        let mut item2=String::new();
        let mut end=String::new();
        br.read_line(&mut item1).unwrap();
        br.read_line(&mut item2).unwrap();
        br.read_line(&mut end).unwrap();
        assert!(item1.contains("501 1"));
        assert!(item2.contains("502 0"));
        assert_eq!(end.trim(),"END");

        // A newer login fences the earlier socket's controller session.
        let mut c=TcpStream::connect(gateway.local_addr()).unwrap();
        c.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let mut cr=BufReader::new(c.try_clone().unwrap());
        assert_eq!(send(&mut c,&mut cr,"AUTH first"),"OK auth");
        assert_eq!(send(&mut a,&mut ar,"INPUT spoof"),"ERR input-rejected");
        assert_eq!(send(&mut c,&mut cr,"JOIN 20"),"OK joined");
        assert_eq!(send(&mut c,&mut cr,"INPUT genuine"),"OK queued");
        assert_eq!(send(&mut c,&mut cr,"QUIT"),"OK bye");
        assert_eq!(shared.lock().unwrap().presence(Id(501)),Some(Id(20)));
    }

    #[test]
    fn server_scheduler_advances_native_world_without_client_driven_ticks() {
        use crate::scheduler::WorldRunner;
        use std::sync::{Arc, Mutex};
        use std::time::Instant;

        let (mut universe,auth)=setup();
        let ticket=universe.connect(&auth,"first").unwrap();
        universe.enter_world(ticket,Id(10)).unwrap();
        universe.input(ticket,text_intent("move")).unwrap();
        let shared=Arc::new(Mutex::new(universe));
        let runner=WorldRunner::start_unchecked_for_test(
            Arc::clone(&shared),Id(10),Duration::from_millis(3),
        ).unwrap();
        let deadline=Instant::now()+Duration::from_secs(3);
        let mut revision=Revision(0);
        while Instant::now() < deadline {
            revision=shared.lock().unwrap().observe(ticket).unwrap()[0].revision;
            if revision.0 > 0 { break; }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(revision,Revision(1));
        assert!(runner.last_error().is_none());
        drop(runner);
        assert_eq!(shared.lock().unwrap().presence(Id(501)),Some(Id(10)));
    }

    #[test]
    fn recovery_uses_native_origin_context_and_preserves_snapshot() {
        let (mut universe,mut auth)=setup();
        let recovered=entity(503,1);
        let saved=Snapshot{
            entity_id:Id(503),revision:Revision(9),binary_artifact:Some(Id(800)),
            state:vec![TypedValue{
                type_ref:TypeRef{
                    namespace:"source".into(),name:"state".into(),version:1,
                },
                data:Value::Bytes(vec![0,1,2,255]),
            }],
        };
        let forged=PersistedPlacement{
            world_instance_id:Id(20),authority_context_id:Id(202),authority_epoch:1,
        };
        assert!(matches!(
            universe.recover_entity(recovered.clone(),Some(&saved),Some(forged)),
            Err(ContractError::StaleAuthority),
        ));
        assert_eq!(universe.entity_count(),2);
        assert_eq!(universe.presence(Id(503)),None);

        universe.recover_entity(recovered.clone(),Some(&saved),Some(PersistedPlacement{
            world_instance_id:Id(20),authority_context_id:Id(201),
            authority_epoch:1,
        })).unwrap();
        let expiry=SystemTime::now()+Duration::from_secs(3600);
        auth.0.insert("restored".into(),Principal{
            user_id:Id(603),player_id:Id(703),
            character_id:Id(503),expires_at:expiry,
        });
        let ticket=universe.connect(&auth,"restored").unwrap();
        let actual=universe.observe(ticket).unwrap();
        assert_eq!(actual.len(),1);
        assert_eq!(actual[0].revision,Revision(9));
        assert_eq!(actual[0].state,saved.state);
        assert_eq!(actual[0].binary_artifact,Some(Id(800)));
        assert_eq!(universe.presence(Id(503)),Some(Id(20)));
        assert!(universe.recover_entity(recovered,Some(&saved),None).is_err());
    }

    #[test]
    fn recovery_rejects_foreign_snapshots_and_allows_unplaced_characters() {
        let (mut universe,_auth)=setup();
        let other=Snapshot{
            entity_id:Id(999),revision:Revision(0),
            state:vec![],binary_artifact:None,
        };
        assert!(universe.recover_entity(entity(700,1),Some(&other),None).is_err());
        universe.recover_entity(entity(700,1),None,None).unwrap();
        assert_eq!(universe.presence(Id(700)),None);
        assert_eq!(universe.entity_count(),3);
    }

}
