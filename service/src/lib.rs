//! Trusted cold-start restoration of universal characters into independently
//! hosted native engine execution contexts. This does not replace origin-game
//! physics, camera, renderer or controller behavior.
//!
//! Only sessions validated by the durable store can authorize restoration.
//! The durable lease MUST be live: an expired lease is not silently treated as
//! an unplaced entity, and cannot authorize native simulation.

use std::error::Error;
use std::fmt;

use oasis_contracts::{ContractError, Id};
use oasis_runtime::universe::{PersistedPlacement, Universe};
use oasis_store::{DurableWorldStore, StoreError};

#[derive(Debug)]
pub enum RecoveryError {
    Storage(StoreError),
    Runtime(ContractError),
    InvalidSession,
    MissingEntity(Id),
    MissingAuthority(Id),
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
}
