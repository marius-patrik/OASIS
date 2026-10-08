//! Trusted cold-start restoration of universal characters into independently
//! hosted native engine execution contexts. This does not replace origin-game
//! physics, camera, renderer or controller behavior.
//!
//! Only sessions validated by the durable store can authorize restoration.
//! The durable lease MUST be live: an expired lease is not silently treated as
//! an unplaced entity, and cannot authorize native simulation.

use std::error::Error;
use std::fmt;
use std::time::SystemTime;
use std::sync::Arc;

use oasis_contracts::{ContractError, ContractResult, Id};
use oasis_runtime::universe::{PersistedPlacement, SimulationLease, Universe};
use oasis_runtime::scheduler::TickAuthorizer;
use oasis_store::{DurableWorldStore, StoreError};

#[derive(Debug)]
pub enum RecoveryError {
    Storage(StoreError),
    Runtime(ContractError),
    InvalidSession,
    MissingEntity(Id),
    MissingAuthority(Id),
    LiveAuthority(Id),
}

impl fmt::Display for RecoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(f,"storage failure: {error}"),
            Self::Runtime(error) => write!(f,"native runtime failure: {error:?}"),
            Self::InvalidSession => write!(f,"session not authorized"),
            Self::MissingEntity(id) => write!(f,"entity {} not found",id.0),
            Self::MissingAuthority(id) => {
                write!(f,"no live authority for character {}",id.0)
            }
            Self::LiveAuthority(id) => {
                write!(f,"character {} still has a live server lease",id.0)
            }
        }
    }
}
impl Error for RecoveryError {}

impl From<StoreError> for RecoveryError {
    fn from(value: StoreError) -> Self { Self::Storage(value) }
}
impl From<ContractError> for RecoveryError {
    fn from(value: ContractError) -> Self { Self::Runtime(value) }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestoredCharacter {
    pub character_id: Id,
    pub world_instance_id: Id,
    pub authority_epoch: u64,
    pub durable_revision: Option<u64>,
    pub native_revision: Option<u64>,
}

/// Production tick gate: verify the active lease before any native module
/// advances or processes controller input. Database failures fail CLOSED.
pub struct PostgresTickAuthorizer {
    store: Arc<dyn DurableWorldStore>,
}

impl PostgresTickAuthorizer {
    pub fn new(store: Arc<dyn DurableWorldStore>) -> Self { Self { store } }
}

impl TickAuthorizer for PostgresTickAuthorizer {
    fn authorize(&self, world: Id, claim: SimulationLease) -> ContractResult<()> {
        let active = self.store.active_presence(claim.entity_id)
            .map_err(|error| ContractError::Internal(
                format!("durable authority check failed: {error}")
            ))?
            .ok_or(ContractError::StaleAuthority)?;
        if active.world_instance_id != world
            || active.authority_context_id != claim.context_id
            || active.authority_epoch != claim.epoch
        {
            return Err(ContractError::StaleAuthority);
        }
        Ok(())
    }
}

pub struct RecoveryService<'a> {
    store: &'a dyn DurableWorldStore,
}

impl<'a> RecoveryService<'a> {
    pub fn new(store: &'a dyn DurableWorldStore) -> Self { Self { store } }

    /// Repopulates the world runtime using the database's original module
    /// identity, state checkpoint, and fenced context. No engine-specific
    /// translation or component substitution is permitted.
    ///
    /// This deliberately fails if no *current* authority lease exists.
    /// Distributed lease reclamation must happen first and will be a
    /// separately auditable, authenticated operation.
    pub fn restore_session(
        &self, universe: &mut Universe, session_id: Id,
    ) -> Result<RestoredCharacter, RecoveryError> {
        let session = self.store.load_session(session_id)?
            .ok_or(RecoveryError::InvalidSession)?;
        let id = session.character_id;
        let entity = self.store.load_entity(id)?
            .ok_or(RecoveryError::MissingEntity(id))?;
        let presence = self.store.active_presence(id)?
            .ok_or(RecoveryError::MissingAuthority(id))?;
        let checkpoint = self.store.latest_state(id)?;
        let native = checkpoint.as_ref().map(|(_,state)| state);
        universe.recover_entity(
            entity, native,
            Some(PersistedPlacement {
                world_instance_id:presence.world_instance_id,
                authority_context_id:presence.authority_context_id,
                authority_epoch:presence.authority_epoch,
            })
        )?;
        Ok(RestoredCharacter {
            character_id:id,
            world_instance_id:presence.world_instance_id,
            authority_epoch:presence.authority_epoch,
            durable_revision:checkpoint.as_ref().map(|(rev,_)|*rev),
            native_revision:native.map(|s|s.revision.0),
        })
    }
    /// After a crash, reassign an EXPIRED authority lease to a new native
    /// execution context. Native module identity and world remain unchanged.
    /// An active lease cannot be preempted.
    ///
    /// A failure during native instantiation leaves the newly issued lease
    /// unusable until it expires; no stale engine becomes authoritative.
    pub fn reclaim_expired_session(
        &self,
        universe: &mut Universe,
        session_id: Id,
        replacement_context: Id,
    ) -> Result<RestoredCharacter, RecoveryError> {
        let session=self.store.load_session(session_id)?
            .ok_or(RecoveryError::InvalidSession)?;
        let character=session.character_id;
        let entity=self.store.load_entity(character)?
            .ok_or(RecoveryError::MissingEntity(character))?;
        if universe.has_entity(character) {
            return Err(RecoveryError::Runtime(ContractError::InvalidData(
                "character already instantiated".into()
            )));
        }
        let recorded=self.store.recorded_presence(character)?
            .ok_or(RecoveryError::MissingAuthority(character))?;
        if recorded.lease_expires_at > SystemTime::now() {
            return Err(RecoveryError::LiveAuthority(character));
        }
        let native=entity.origin_module
            .ok_or(RecoveryError::Runtime(ContractError::InvalidData(
                "origin module missing".into()
            )))?;
        if universe.context_for(recorded.world_instance_id,native)
            != Some(replacement_context) {
            return Err(RecoveryError::Runtime(ContractError::StaleAuthority));
        }
        self.store.reclaim_expired_authority(
            session_id,character,replacement_context,recorded.authority_epoch
        )?;
        self.restore_session(universe,session_id)
    }

}
