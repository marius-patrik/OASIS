//! Durable SQL boundary for portable characters and native engine state.
//! Native engines never access SQL. Trusted server code supplies verified
//! session IDs and monotonic expected revisions to this store.
//!
//! This synchronous PostgreSQL client is suitable for internal development
//! and integration tests. Use TLS and a restricted database role in deployment.

pub mod codec;

use std::io;
use std::sync::{Mutex, MutexGuard};
use std::time::SystemTime;

use oasis_contracts::{
    DefinitionRef, EntityView, Id, Revision, Snapshot, TypeRef, TypedValue,
};
use postgres::{Client, NoTls};
use serde_json::Value as Json;
use uuid::Uuid;

pub type StoreError = Box<dyn std::error::Error + Send + Sync>;
pub type StoreResult<T> = Result<T, StoreError>;

pub(crate) fn invalid(message: &str) -> StoreError {
    io::Error::new(io::ErrorKind::InvalidData, message).into()
}
fn to_uuid(id: Id) -> Uuid { Uuid::from_u128(id.0) }
fn from_uuid(id: Uuid) -> Id { Id(id.as_u128()) }

#[derive(Clone, Debug)]
pub struct StoredSession {
    pub user_id: Id,
    pub player_id: Id,
    pub character_id: Id,
    pub expires_at: SystemTime,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivePresence {
    pub presence_id: Id,
    pub world_instance_id: Id,
    pub frame_id: Id,
    pub authority_context_id: Id,
    pub authority_epoch: u64,
    pub lease_expires_at: SystemTime,
}

#[derive(Clone, Debug)]
pub struct TravelCommand {
    pub transaction_id: Id,
    pub idempotency_key: String,
    pub session_id: Id,
    pub character_id: Id,
    pub expected_presence_id: Option<Id>,
    pub destination_presence_id: Id,
    pub destination_world_id: Id,
    pub destination_frame_id: Id,
    pub destination_context_id: Id,
    pub expected_authority_epoch: u64,
    pub event_type_id: Id,
    pub snapshot_id: Id,
    pub snapshot_type_id: Id,
    /// Monotonic durable revision, independent of engine-native tick/revision.
    pub expected_snapshot_revision: u64,
    pub native_state: Snapshot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TravelReceipt {
    pub transaction_id: Id,
    pub authority_epoch: u64,
    pub snapshot_revision: u64,
    pub applied: bool,
}

/// Repository interface usable by the MMO coordinator without referencing
/// PostgreSQL, SQL strings, or a particular game.
pub trait DurableWorldStore: Send + Sync {
    fn load_session(&self, session_id: Id) -> StoreResult<Option<StoredSession>>;
    fn load_entity(&self, entity_id: Id) -> StoreResult<Option<EntityView>>;
    /// Read persisted world location even if its lease has expired.
    fn recorded_presence(&self, entity_id: Id) -> StoreResult<Option<ActivePresence>>;
    /// Returns only live, nonexpired authority.
    fn active_presence(&self, entity_id: Id) -> StoreResult<Option<ActivePresence>>;
    fn renew_authority(&self, session: Id, character: Id, context: Id, epoch: u64)
        -> StoreResult<u64>;
    fn reclaim_expired_authority(
        &self, session: Id, character: Id, context: Id, epoch: u64,
    ) -> StoreResult<u64>;
    fn latest_state(&self, entity_id: Id) -> StoreResult<Option<(u64, Snapshot)>>;
    fn transfer_character(&self, command: &TravelCommand) -> StoreResult<TravelReceipt>;
}

pub struct PostgresStore {
    client: Mutex<Client>,
}

impl PostgresStore {
    pub fn connect(database_url: &str) -> StoreResult<Self> {
        Ok(Self { client: Mutex::new(Client::connect(database_url, NoTls)?) })
    }
    fn connection(&self) -> StoreResult<MutexGuard<'_, Client>> {
        Ok(self.client.lock().map_err(|_|io::Error::other("store lock poisoned"))?)
    }
}

impl DurableWorldStore for PostgresStore {
    fn load_session(&self, session_id: Id) -> StoreResult<Option<StoredSession>> {
        let row=self.connection()?.query_opt(
            "SELECT s.user_id,s.player_id,s.active_character_id,s.expires_at
               FROM sessions s JOIN users u ON u.id=s.user_id
              WHERE s.id=$1 AND s.status='active' AND s.expires_at>now()
                AND u.status='active' AND s.active_character_id IS NOT NULL",
            &[&to_uuid(session_id)],
        )?;
        Ok(row.map(|r| StoredSession {
            user_id:from_uuid(r.get::<_,Uuid>(0)),
            player_id:from_uuid(r.get::<_,Uuid>(1)),
            character_id:from_uuid(r.get::<_,Uuid>(2)),
            expires_at:r.get::<_,SystemTime>(3),
        }))
    }

    fn load_entity(&self, entity_id: Id) -> StoreResult<Option<EntityView>> {
        let mut client=self.connection()?;
        let row=client.query_opt(
            "SELECT e.definition_id,d.version,e.revision,e.origin_module_id
               FROM entities e
               JOIN entity_definitions d ON d.id=e.definition_id
              WHERE e.id=$1",
            &[&to_uuid(entity_id)],
        )?;
        let Some(row)=row else {return Ok(None)};
        let version: i32=row.get(1);
        let revision:i64=row.get(2);
        let origin:Option<Uuid>=row.get(3);
        let rows=client.query(
            "SELECT t.namespace,t.name,t.version,c.data
               FROM components c JOIN types t ON t.id=c.type_id
              WHERE c.entity_id=$1 AND c.presence_id IS NULL
              ORDER BY t.namespace,t.name,t.version,c.slot",
            &[&to_uuid(entity_id)],
        )?;
        let mut components=Vec::new();
        for c in rows {
            let type_version:i32=c.get(2);
            let data:Json=c.get(3);
            components.push(TypedValue{
                type_ref:TypeRef {
                    namespace:c.get(0), name:c.get(1),
                    version:u32::try_from(type_version)?,
                },
                data:codec::json_component(&data)?,
            });
        }
        Ok(Some(EntityView {
            id:entity_id,
            definition:DefinitionRef {
                id:from_uuid(row.get::<_,Uuid>(0)),
                version:u32::try_from(version)?,
            },
            revision:Revision(u64::try_from(revision)?),
            components,
            origin_module:origin.map(from_uuid),
        }))
    }

    fn recorded_presence(&self, entity_id: Id) -> StoreResult<Option<ActivePresence>> {
        let resource_key=format!("character:{}",to_uuid(entity_id));
        let row=self.connection()?.query_opt(
            "SELECT p.id,p.world_instance_id,p.frame_id,a.holder_context_id,
                    a.epoch,a.expires_at
               FROM presences p JOIN authority_leases a ON a.resource_key=$2
              WHERE p.entity_id=$1 AND p.active",
            &[&to_uuid(entity_id),&resource_key],
        )?;
        let Some(row)=row else{return Ok(None)};
        let epoch:i64=row.get(4);
        Ok(Some(ActivePresence {
            presence_id:from_uuid(row.get(0)),
            world_instance_id:from_uuid(row.get(1)),
            frame_id:from_uuid(row.get(2)),
            authority_context_id:from_uuid(row.get(3)),
            authority_epoch:u64::try_from(epoch)?,
            lease_expires_at:row.get(5),
        }))
    }

    fn active_presence(&self, entity_id: Id) -> StoreResult<Option<ActivePresence>> {
        Ok(self.recorded_presence(entity_id)?
            .filter(|p|p.lease_expires_at>SystemTime::now()))
    }

    fn renew_authority(
        &self, session: Id, character: Id, context: Id, epoch: u64,
    ) -> StoreResult<u64> {
        let expected=i64::try_from(epoch)?;
        let row=self.connection()?.query_one(
            "SELECT renew_character_authority($1,$2,$3,$4)",
            &[&to_uuid(session),&to_uuid(character),&to_uuid(context),&expected],
        )?;
        let next:i64=row.get(0);
        Ok(u64::try_from(next)?)
    }

    fn reclaim_expired_authority(
        &self, session: Id, character: Id, context: Id, epoch: u64,
    ) -> StoreResult<u64> {
        let expected=i64::try_from(epoch)?;
        let row=self.connection()?.query_one(
            "SELECT reclaim_character_authority($1,$2,$3,$4)",
            &[&to_uuid(session),&to_uuid(character),&to_uuid(context),&expected],
        )?;
        let next:i64=row.get(0);
        Ok(u64::try_from(next)?)
    }

    fn latest_state(&self, entity_id: Id) -> StoreResult<Option<(u64,Snapshot)>> {
        let row=self.connection()?.query_opt(
            "SELECT revision,data FROM state_documents
              WHERE entity_id=$1 ORDER BY revision DESC LIMIT 1",
            &[&to_uuid(entity_id)],
        )?;
        let Some(row)=row else{return Ok(None)};
        let version:i64=row.get(0);
        let data:Json=row.get(1);
        Ok(Some((u64::try_from(version)?,codec::decode_snapshot(entity_id,&data)?)))
    }

    fn transfer_character(&self, command: &TravelCommand) -> StoreResult<TravelReceipt> {
        if command.native_state.entity_id != command.character_id {
            return Err(invalid("travel snapshot must belong to transported character"));
        }
        if command.idempotency_key.is_empty() {
            return Err(invalid("idempotency key required"));
        }
        let expected_epoch=i64::try_from(command.expected_authority_epoch)?;
        let expected_version=i64::try_from(command.expected_snapshot_revision)?;
        let native_json=codec::encode_snapshot(&command.native_state)?;
        let old_presence=command.expected_presence_id.map(to_uuid);
        let row=self.connection()?.query_one(
            "SELECT result_transaction_id,authority_epoch,was_applied,snapshot_revision
               FROM persist_character_travel(
                 $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15
               )",
            &[
                &to_uuid(command.transaction_id),
                &command.idempotency_key,
                &to_uuid(command.session_id),
                &to_uuid(command.character_id),
                &old_presence,
                &to_uuid(command.destination_presence_id),
                &to_uuid(command.destination_world_id),
                &to_uuid(command.destination_frame_id),
                &to_uuid(command.destination_context_id),
                &expected_epoch,
                &to_uuid(command.event_type_id),
                &to_uuid(command.snapshot_id),
                &to_uuid(command.snapshot_type_id),
                &expected_version,
                &native_json,
            ],
        )?;
        let epoch:i64=row.get(1);
        let version:i64=row.get(3);
        Ok(TravelReceipt {
            transaction_id:from_uuid(row.get(0)),
            authority_epoch:u64::try_from(epoch)?,
            applied:row.get(2),
            snapshot_revision:u64::try_from(version)?,
        })
    }
}
