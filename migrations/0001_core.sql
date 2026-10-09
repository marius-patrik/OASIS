-- OASIS universal relational core, v0.1; target PostgreSQL 15+.
-- Apply in one transaction. API-layer registry validation is required in addition to SQL constraints.
BEGIN;

CREATE TABLE types (
  id UUID PRIMARY KEY,
  namespace TEXT NOT NULL,
  name TEXT NOT NULL,
  version INTEGER NOT NULL CHECK (version > 0),
  category TEXT NOT NULL,
  schema JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(schema) = 'object'),
  supersedes_id UUID REFERENCES types(id),
  data JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(data) = 'object'),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (namespace, name, version),
  CHECK (namespace <> '' AND name <> '' AND category <> ''),
  CHECK (supersedes_id IS DISTINCT FROM id)
);

CREATE TABLE assets (
  id UUID PRIMARY KEY,
  content_hash TEXT NOT NULL UNIQUE,
  media_type TEXT NOT NULL,
  byte_length BIGINT NOT NULL CHECK (byte_length >= 0),
  storage_uri TEXT NOT NULL,
  data JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(data) = 'object'),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  CHECK (content_hash <> '' AND storage_uri <> '')
);

CREATE TABLE users (
  id UUID PRIMARY KEY,
  auth_subject TEXT NOT NULL UNIQUE,
  status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','suspended','closed')),
  data JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(data) = 'object'),
  revision BIGINT NOT NULL DEFAULT 0 CHECK (revision >= 0),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE engines (
  id UUID PRIMARY KEY,
  namespace TEXT NOT NULL,
  name TEXT NOT NULL,
  version TEXT NOT NULL,
  data JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(data) = 'object'),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE(namespace, name, version)
);

CREATE TABLE engine_modules (
  id UUID PRIMARY KEY,
  engine_id UUID NOT NULL REFERENCES engines(id),
  module_kind TEXT NOT NULL,
  version TEXT NOT NULL,
  artifact_id UUID NOT NULL REFERENCES assets(id),
  interface_manifest JSONB NOT NULL DEFAULT '{}'::jsonb
    CHECK (jsonb_typeof(interface_manifest) = 'object'),
  data JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(data) = 'object'),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE engine_adapters (
  id UUID PRIMARY KEY,
  engine_id UUID NOT NULL REFERENCES engines(id),
  version TEXT NOT NULL,
  artifact_id UUID NOT NULL REFERENCES assets(id),
  contract_version TEXT NOT NULL,
  declared_interfaces JSONB NOT NULL DEFAULT '{}'::jsonb
    CHECK (jsonb_typeof(declared_interfaces) = 'object'),
  data JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(data) = 'object'),
  UNIQUE(engine_id, version)
);

CREATE TABLE entity_definitions (
  id UUID PRIMARY KEY,
  type_id UUID NOT NULL REFERENCES types(id),
  namespace TEXT NOT NULL,
  name TEXT NOT NULL,
  version INTEGER NOT NULL CHECK (version > 0),
  origin_module_id UUID REFERENCES engine_modules(id),
  data JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(data) = 'object'),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE(namespace, name, version)
);

CREATE TABLE entities (
  id UUID PRIMARY KEY,
  definition_id UUID NOT NULL REFERENCES entity_definitions(id),
  origin_module_id UUID REFERENCES engine_modules(id),
  data JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(data) = 'object'),
  revision BIGINT NOT NULL DEFAULT 0 CHECK (revision >= 0),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE players (
  id UUID PRIMARY KEY,
  user_id UUID NOT NULL REFERENCES users(id),
  entity_id UUID NOT NULL UNIQUE REFERENCES entities(id),
  handle TEXT NOT NULL CHECK (char_length(handle) BETWEEN 3 AND 64),
  data JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(data) = 'object'),
  revision BIGINT NOT NULL DEFAULT 0 CHECK (revision >= 0),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE(id, user_id)
);

CREATE TABLE characters (
  entity_id UUID PRIMARY KEY REFERENCES entities(id),
  player_id UUID NOT NULL REFERENCES players(id),
  data JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(data) = 'object'),
  UNIQUE(entity_id, player_id)
  -- definition_id is authoritative in entities, not duplicated here.
);

CREATE TABLE sessions (
  id UUID PRIMARY KEY,
  user_id UUID NOT NULL REFERENCES users(id),
  player_id UUID NOT NULL,
  active_character_id UUID,
  status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','revoked','expired')),
  expires_at TIMESTAMPTZ NOT NULL,
  data JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(data) = 'object'),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  FOREIGN KEY (player_id,user_id) REFERENCES players(id,user_id),
  FOREIGN KEY (active_character_id,player_id)
    REFERENCES characters(entity_id,player_id)
);

CREATE TABLE games (
  id UUID PRIMARY KEY,
  name TEXT NOT NULL,
  default_engine_id UUID REFERENCES engines(id),
  data JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(data) = 'object'),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE worlds (
  id UUID PRIMARY KEY,
  game_id UUID NOT NULL REFERENCES games(id),
  definition_id UUID REFERENCES entity_definitions(id),
  data JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(data) = 'object'),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE world_modules (
  world_id UUID NOT NULL REFERENCES worlds(id),
  module_id UUID NOT NULL REFERENCES engine_modules(id),
  role TEXT NOT NULL,
  settings JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(settings) = 'object'),
  PRIMARY KEY(world_id,module_id,role)
);

CREATE TABLE world_instances (
  id UUID PRIMARY KEY,
  world_id UUID NOT NULL REFERENCES worlds(id),
  status TEXT NOT NULL DEFAULT 'starting'
    CHECK (status IN ('starting','running','paused','stopping','stopped','failed')),
  authority_node TEXT,
  checkpoint_id UUID, -- circular FK added after state_documents
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  data JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(data) = 'object')
);

CREATE TABLE spatial_frames (
  id UUID PRIMARY KEY,
  world_id UUID REFERENCES worlds(id),
  parent_id UUID REFERENCES spatial_frames(id),
  dimensions SMALLINT NOT NULL CHECK (dimensions BETWEEN 1 AND 4),
  basis JSONB NOT NULL CHECK (jsonb_typeof(basis) = 'object'),
  unit_scale NUMERIC NOT NULL DEFAULT 1 CHECK (unit_scale > 0),
  data JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(data) = 'object'),
  CHECK (parent_id IS DISTINCT FROM id)
);

CREATE TABLE presences (
  id UUID PRIMARY KEY,
  entity_id UUID NOT NULL REFERENCES entities(id),
  world_instance_id UUID NOT NULL REFERENCES world_instances(id),
  frame_id UUID NOT NULL REFERENCES spatial_frames(id),
  active BOOLEAN NOT NULL DEFAULT TRUE,
  transform JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(transform) = 'object'),
  data JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(data) = 'object'),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  ended_at TIMESTAMPTZ,
  UNIQUE(id, entity_id),
  CHECK ((active AND ended_at IS NULL) OR NOT active)
);

CREATE TABLE items (
  entity_id UUID PRIMARY KEY REFERENCES entities(id),
  data JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(data) = 'object')
);

CREATE TABLE inventories (
  entity_id UUID PRIMARY KEY REFERENCES entities(id),
  owner_entity_id UUID NOT NULL REFERENCES entities(id),
  data JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(data) = 'object')
);

CREATE TABLE components (
  id UUID PRIMARY KEY,
  entity_id UUID NOT NULL REFERENCES entities(id),
  type_id UUID NOT NULL REFERENCES types(id),
  presence_id UUID,
  slot TEXT NOT NULL DEFAULT 'default',
  -- Source-native components use the same lossless tagged Value codec as
  -- durable snapshots, rather than ambiguous untyped JSON.
  data JSONB NOT NULL DEFAULT '{"k":"map","v":{}}'::jsonb
    CHECK (jsonb_typeof(data) = 'object'
       AND data ? 'k'
       AND data->>'k' IN ('null','bool','int','uint','float',
                         'string','bytes','ref','sequence','map')),
  revision BIGINT NOT NULL DEFAULT 0 CHECK (revision >= 0),
  FOREIGN KEY(presence_id, entity_id) REFERENCES presences(id, entity_id),
  UNIQUE NULLS NOT DISTINCT(entity_id, type_id, presence_id, slot)
);

CREATE TABLE capabilities (
  id UUID PRIMARY KEY,
  name TEXT NOT NULL,
  type_id UUID NOT NULL REFERENCES types(id),
  module_id UUID NOT NULL REFERENCES engine_modules(id),
  entrypoint TEXT NOT NULL,
  interface_schema JSONB NOT NULL DEFAULT '{}'::jsonb
    CHECK (jsonb_typeof(interface_schema) = 'object'),
  data JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(data) = 'object'),
  CHECK (entrypoint <> '')
);

CREATE TABLE entity_capabilities (
  entity_id UUID NOT NULL REFERENCES entities(id),
  capability_id UUID NOT NULL REFERENCES capabilities(id),
  binding JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(binding) = 'object'),
  PRIMARY KEY(entity_id,capability_id)
);

CREATE TABLE relations (
  id UUID PRIMARY KEY,
  type_id UUID NOT NULL REFERENCES types(id),
  source_entity_id UUID NOT NULL REFERENCES entities(id),
  target_entity_id UUID NOT NULL REFERENCES entities(id),
  data JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(data) = 'object')
);

CREATE TABLE ownerships (
  entity_id UUID PRIMARY KEY REFERENCES entities(id),
  owner_entity_id UUID NOT NULL REFERENCES entities(id),
  revision BIGINT NOT NULL DEFAULT 0 CHECK (revision >= 0),
  CHECK (entity_id <> owner_entity_id)
);

CREATE TABLE locations (
  entity_id UUID PRIMARY KEY REFERENCES entities(id),
  container_entity_id UUID REFERENCES inventories(entity_id),
  presence_id UUID,
  revision BIGINT NOT NULL DEFAULT 0 CHECK (revision >= 0),
  CHECK ((container_entity_id IS NULL) <> (presence_id IS NULL)),
  CHECK (container_entity_id IS DISTINCT FROM entity_id),
  FOREIGN KEY(presence_id, entity_id) REFERENCES presences(id,entity_id)
);

CREATE TABLE execution_contexts (
  id UUID PRIMARY KEY,
  world_instance_id UUID NOT NULL REFERENCES world_instances(id),
  module_id UUID NOT NULL REFERENCES engine_modules(id),
  status TEXT NOT NULL DEFAULT 'initializing'
    CHECK (status IN ('initializing','running','suspended','failed','destroyed')),
  authority_epoch BIGINT NOT NULL DEFAULT 0 CHECK (authority_epoch >= 0),
  data JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(data) = 'object'),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE engine_bindings (
  id UUID PRIMARY KEY,
  definition_id UUID NOT NULL REFERENCES entity_definitions(id),
  module_id UUID NOT NULL REFERENCES engine_modules(id),
  native_type_key TEXT NOT NULL,
  data JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(data) = 'object'),
  UNIQUE(definition_id,module_id,native_type_key)
);

CREATE TABLE state_documents (
  id UUID PRIMARY KEY,
  type_id UUID NOT NULL REFERENCES types(id),
  entity_id UUID REFERENCES entities(id),
  world_instance_id UUID REFERENCES world_instances(id),
  data JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(data) = 'object'),
  artifact_id UUID REFERENCES assets(id),
  revision BIGINT NOT NULL CHECK (revision >= 0),
  checkpoint_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  CHECK ((entity_id IS NULL) <> (world_instance_id IS NULL))
);

ALTER TABLE world_instances ADD CONSTRAINT world_instances_checkpoint_fk
  FOREIGN KEY(checkpoint_id) REFERENCES state_documents(id) DEFERRABLE INITIALLY IMMEDIATE;

CREATE TABLE authority_leases (
  resource_key TEXT PRIMARY KEY,
  holder_context_id UUID NOT NULL REFERENCES execution_contexts(id),
  epoch BIGINT NOT NULL CHECK (epoch > 0),
  expires_at TIMESTAMPTZ NOT NULL,
  CHECK(resource_key <> '')
);

CREATE TABLE transactions (
  id UUID PRIMARY KEY,
  actor_entity_id UUID REFERENCES entities(id),
  idempotency_key TEXT NOT NULL UNIQUE,
  status TEXT NOT NULL CHECK (status IN ('pending','committed','rejected')),
  committed_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  CHECK ((status = 'committed') = (committed_at IS NOT NULL))
);

CREATE TABLE events (
  id UUID PRIMARY KEY,
  transaction_id UUID NOT NULL REFERENCES transactions(id),
  sequence BIGINT GENERATED ALWAYS AS IDENTITY UNIQUE,
  subject_entity_id UUID REFERENCES entities(id),
  type_id UUID NOT NULL REFERENCES types(id),
  payload JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(payload) = 'object'),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMIT;