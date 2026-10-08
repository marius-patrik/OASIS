-- End-to-end relational smoke test against a disposable PostgreSQL 16 server.
BEGIN;

INSERT INTO types (id, namespace, name, version, category, schema)
VALUES ('00000000-0000-0000-0000-000000000001', 'test', 'base', 1, 'entity', '{}');

INSERT INTO users (id, auth_subject, data)
VALUES ('00000000-0000-0000-0000-000000000002', 'ci-subject', '{"name":"test"}');

INSERT INTO engines (id, namespace, name, version)
VALUES ('00000000-0000-0000-0000-000000000003', 'test', 'fixture', '1');

INSERT INTO assets (id, content_hash, media_type, byte_length, storage_uri)
VALUES ('00000000-0000-0000-0000-000000000004', 'sha256:ci', 'application/octet-stream', 1, 'memory://ci');

INSERT INTO engine_modules (id, engine_id, module_kind, version, artifact_id)
VALUES ('00000000-0000-0000-0000-000000000005',
        '00000000-0000-0000-0000-000000000003', 'simulation', '1',
        '00000000-0000-0000-0000-000000000004');

INSERT INTO entity_definitions (id, type_id, namespace, name, version, origin_module_id)
VALUES ('00000000-0000-0000-0000-000000000006',
        '00000000-0000-0000-0000-000000000001', 'test', 'actor', 1,
        '00000000-0000-0000-0000-000000000005');

INSERT INTO entities (id, definition_id, origin_module_id, data)
VALUES
('00000000-0000-0000-0000-000000000007',
 '00000000-0000-0000-0000-000000000006',
 '00000000-0000-0000-0000-000000000005', '{"arbitrary":{"dynamic":true}}'),
('00000000-0000-0000-0000-000000000008',
 '00000000-0000-0000-0000-000000000006',
 '00000000-0000-0000-0000-000000000005', '{"ammunition":8}');

INSERT INTO players (id, user_id, entity_id, handle)
VALUES ('00000000-0000-0000-0000-000000000009',
        '00000000-0000-0000-0000-000000000002',
        '00000000-0000-0000-0000-000000000007', 'fixture-player');

INSERT INTO characters (entity_id, player_id, data)
VALUES ('00000000-0000-0000-0000-000000000007',
        '00000000-0000-0000-0000-000000000009', '{"origin":"test"}');

INSERT INTO items (entity_id, data)
VALUES ('00000000-0000-0000-0000-000000000008', '{"native":"weapon"}');

INSERT INTO inventories (entity_id, owner_entity_id)
VALUES ('00000000-0000-0000-0000-000000000007',
        '00000000-0000-0000-0000-000000000007');

INSERT INTO ownerships (entity_id, owner_entity_id)
VALUES ('00000000-0000-0000-0000-000000000008',
        '00000000-0000-0000-0000-000000000007');

INSERT INTO locations (entity_id, container_entity_id)
VALUES ('00000000-0000-0000-0000-000000000008',
        '00000000-0000-0000-0000-000000000007');

INSERT INTO components (id, entity_id, type_id, data)
VALUES ('00000000-0000-0000-0000-000000000010',
        '00000000-0000-0000-0000-000000000007',
        '00000000-0000-0000-0000-000000000001',
        '{"gameSpecific": [1,2,3], "arbitrary": {"nesting": 5}}');

INSERT INTO games (id, name, default_engine_id)
VALUES ('00000000-0000-0000-0000-000000000011', 'Fixture',
        '00000000-0000-0000-0000-000000000003');
INSERT INTO worlds (id, game_id)
VALUES ('00000000-0000-0000-0000-000000000012',
        '00000000-0000-0000-0000-000000000011');
INSERT INTO world_instances (id, world_id, status)
VALUES ('00000000-0000-0000-0000-000000000013',
        '00000000-0000-0000-0000-000000000012', 'running');

INSERT INTO spatial_frames (id, world_id, dimensions, basis)
VALUES ('00000000-0000-0000-0000-000000000014',
        '00000000-0000-0000-0000-000000000012', 2, '{"axes":["x","y"]}');

INSERT INTO presences (id, entity_id, world_instance_id, frame_id)
VALUES ('00000000-0000-0000-0000-000000000015',
        '00000000-0000-0000-0000-000000000007',
        '00000000-0000-0000-0000-000000000013',
        '00000000-0000-0000-0000-000000000014');

INSERT INTO locations (entity_id, presence_id)
VALUES ('00000000-0000-0000-0000-000000000007',
        '00000000-0000-0000-0000-000000000015');

DO $$
BEGIN
  IF (SELECT data #>> '{arbitrary,dynamic}' FROM entities
      WHERE id = '00000000-0000-0000-0000-000000000007') <> 'true' THEN
    RAISE EXCEPTION 'Extensible record was not preserved';
  END IF;
  IF (SELECT count(*) FROM items) <> 1 THEN
    RAISE EXCEPTION 'Item did not persist';
  END IF;
  IF (SELECT count(*) FROM presences WHERE active) <> 1 THEN
    RAISE EXCEPTION 'Presence was not created';
  END IF;
  BEGIN
    INSERT INTO locations (entity_id, container_entity_id)
    VALUES ('00000000-0000-0000-0000-000000000008',
            '00000000-0000-0000-0000-000000000007');
    RAISE EXCEPTION 'Duplicate item custody was permitted';
  EXCEPTION WHEN unique_violation THEN NULL;
  END;
END $$;


-- A second owner and inventory receive the exact same item, without
-- duplicating it or replacing its native definition or state.
INSERT INTO entities (id, definition_id, origin_module_id)
VALUES ('00000000-0000-0000-0000-000000000016',
        '00000000-0000-0000-0000-000000000006',
        '00000000-0000-0000-0000-000000000005');
INSERT INTO inventories (entity_id, owner_entity_id)
VALUES ('00000000-0000-0000-0000-000000000016',
        '00000000-0000-0000-0000-000000000016');

DO $$
DECLARE
  changed BOOLEAN;
  replayed BOOLEAN;
BEGIN
  SELECT was_applied INTO changed FROM transfer_item(
    '00000000-0000-0000-0000-000000000017',
    'ci-transfer-1',
    '00000000-0000-0000-0000-000000000007',
    '00000000-0000-0000-0000-000000000008',
    '00000000-0000-0000-0000-000000000007',
    '00000000-0000-0000-0000-000000000016',
    '00000000-0000-0000-0000-000000000016',
    NULL,
    0,
    '00000000-0000-0000-0000-000000000001'
  );
  IF NOT changed THEN RAISE EXCEPTION 'first transfer was not applied'; END IF;
  SELECT was_applied INTO replayed FROM transfer_item(
    '00000000-0000-0000-0000-000000000017',
    'ci-transfer-1',
    '00000000-0000-0000-0000-000000000007',
    '00000000-0000-0000-0000-000000000008',
    '00000000-0000-0000-0000-000000000007',
    '00000000-0000-0000-0000-000000000016',
    '00000000-0000-0000-0000-000000000016',
    NULL,
    0,
    '00000000-0000-0000-0000-000000000001'
  );
  IF replayed THEN RAISE EXCEPTION 'duplicate transfer was applied twice'; END IF;
  IF (SELECT owner_entity_id FROM ownerships
       WHERE entity_id = '00000000-0000-0000-0000-000000000008')
       <> '00000000-0000-0000-0000-000000000016' THEN
    RAISE EXCEPTION 'wrong item owner after transfer';
  END IF;
  IF (SELECT count(*) FROM events WHERE transaction_id =
      '00000000-0000-0000-0000-000000000017') <> 1 THEN
    RAISE EXCEPTION 'transfer event is not unique';
  END IF;
  IF (SELECT revision FROM locations
       WHERE entity_id='00000000-0000-0000-0000-000000000008') <> 1 THEN
    RAISE EXCEPTION 'location revision incorrectly advanced';
  END IF;
  BEGIN
    PERFORM * FROM transfer_item(
      '00000000-0000-0000-0000-000000000018',
      'ci-transfer-stale', '00000000-0000-0000-0000-000000000007',
      '00000000-0000-0000-0000-000000000008',
      '00000000-0000-0000-0000-000000000007',
      '00000000-0000-0000-0000-000000000016',
      '00000000-0000-0000-0000-000000000016',
      NULL, 0, '00000000-0000-0000-0000-000000000001'
    );
    RAISE EXCEPTION 'stale owner accepted';
  EXCEPTION WHEN serialization_failure THEN NULL;
  END;
END $$;


-- Durable character travel: same entity across independently hosted worlds.
INSERT INTO sessions(id,user_id,player_id,active_character_id,status,expires_at)
VALUES('00000000-0000-0000-0000-000000000021',
       '00000000-0000-0000-0000-000000000002',
       '00000000-0000-0000-0000-000000000009',
       '00000000-0000-0000-0000-000000000007','active',now()+INTERVAL '1 hour');
INSERT INTO execution_contexts(id,world_instance_id,module_id,status)
VALUES('00000000-0000-0000-0000-000000000020',
       '00000000-0000-0000-0000-000000000013',
       '00000000-0000-0000-0000-000000000005','running');
INSERT INTO worlds(id,game_id)
VALUES('00000000-0000-0000-0000-000000000022',
       '00000000-0000-0000-0000-000000000011');
INSERT INTO world_instances(id,world_id,status)
VALUES('00000000-0000-0000-0000-000000000023',
       '00000000-0000-0000-0000-000000000022','running');
INSERT INTO spatial_frames(id,world_id,dimensions,basis)
VALUES('00000000-0000-0000-0000-000000000024',
       '00000000-0000-0000-0000-000000000022',3,'{"axes":["x","y","z"]}');
INSERT INTO execution_contexts(id,world_instance_id,module_id,status)
VALUES('00000000-0000-0000-0000-000000000025',
       '00000000-0000-0000-0000-000000000023',
       '00000000-0000-0000-0000-000000000005','running');
INSERT INTO authority_leases(resource_key,holder_context_id,epoch,expires_at)
VALUES('character:00000000-0000-0000-0000-000000000007',
       '00000000-0000-0000-0000-000000000020',1,now()+INTERVAL '1 minute');

DO $$
DECLARE
  did_apply BOOLEAN;
  epoch_after BIGINT;
BEGIN
  SELECT new_authority_epoch, was_applied
    INTO epoch_after, did_apply
    FROM transfer_character_presence(
      '00000000-0000-0000-0000-000000000027', 'travel-ci-1',
      '00000000-0000-0000-0000-000000000021',
      '00000000-0000-0000-0000-000000000007',
      '00000000-0000-0000-0000-000000000015',
      '00000000-0000-0000-0000-000000000026',
      '00000000-0000-0000-0000-000000000023',
      '00000000-0000-0000-0000-000000000024',
      '00000000-0000-0000-0000-000000000025',1,
      '00000000-0000-0000-0000-000000000001'
    );
  IF NOT did_apply OR epoch_after <> 2 THEN
    RAISE EXCEPTION 'world travel was not applied with epoch 2';
  END IF;
  IF (SELECT count(*) FROM presences WHERE entity_id =
      '00000000-0000-0000-0000-000000000007' AND active) <> 1 THEN
    RAISE EXCEPTION 'one character has multiple active world presences';
  END IF;
  IF (SELECT world_instance_id FROM presences
       WHERE entity_id='00000000-0000-0000-0000-000000000007' AND active)
       <> '00000000-0000-0000-0000-000000000023'::uuid THEN
    RAISE EXCEPTION 'character landed in wrong world';
  END IF;
  IF (SELECT holder_context_id FROM authority_leases
       WHERE resource_key='character:00000000-0000-0000-0000-000000000007')
       <> '00000000-0000-0000-0000-000000000025'::uuid THEN
    RAISE EXCEPTION 'character authority did not move to destination context';
  END IF;
  SELECT was_applied INTO did_apply FROM transfer_character_presence(
      '00000000-0000-0000-0000-000000000027', 'travel-ci-1',
      '00000000-0000-0000-0000-000000000021',
      '00000000-0000-0000-0000-000000000007',
      '00000000-0000-0000-0000-000000000015',
      '00000000-0000-0000-0000-000000000026',
      '00000000-0000-0000-0000-000000000023',
      '00000000-0000-0000-0000-000000000024',
      '00000000-0000-0000-0000-000000000025',1,
      '00000000-0000-0000-0000-000000000001'
    );
  IF did_apply THEN RAISE EXCEPTION 'travel replay changed state'; END IF;
  -- Returning home must advance the epoch; replaying the original travel
  -- must report the original commit rather than today's fencing token.
  SELECT new_authority_epoch, was_applied INTO epoch_after, did_apply
  FROM transfer_character_presence(
      '00000000-0000-0000-0000-000000000030', 'travel-ci-return',
      '00000000-0000-0000-0000-000000000021',
      '00000000-0000-0000-0000-000000000007',
      '00000000-0000-0000-0000-000000000026',
      '00000000-0000-0000-0000-000000000029',
      '00000000-0000-0000-0000-000000000013',
      '00000000-0000-0000-0000-000000000014',
      '00000000-0000-0000-0000-000000000020',2,
      '00000000-0000-0000-0000-000000000001'
  );
  IF NOT did_apply OR epoch_after <> 3 THEN
    RAISE EXCEPTION 'return travel did not advance epoch to 3';
  END IF;
  SELECT new_authority_epoch, was_applied INTO epoch_after, did_apply
  FROM transfer_character_presence(
      '00000000-0000-0000-0000-000000000027', 'travel-ci-1',
      '00000000-0000-0000-0000-000000000021',
      '00000000-0000-0000-0000-000000000007',
      '00000000-0000-0000-0000-000000000015',
      '00000000-0000-0000-0000-000000000026',
      '00000000-0000-0000-0000-000000000023',
      '00000000-0000-0000-0000-000000000024',
      '00000000-0000-0000-0000-000000000025',1,
      '00000000-0000-0000-0000-000000000001'
  );
  IF did_apply OR epoch_after <> 2 THEN
    RAISE EXCEPTION 'idempotent replay did not preserve original authority epoch';
  END IF;
  BEGIN
    PERFORM * FROM transfer_character_presence(
      '00000000-0000-0000-0000-000000000028','travel-stale',
      '00000000-0000-0000-0000-000000000021',
      '00000000-0000-0000-0000-000000000007',
      '00000000-0000-0000-0000-000000000015',
      '00000000-0000-0000-0000-000000000029',
      '00000000-0000-0000-0000-000000000023',
      '00000000-0000-0000-0000-000000000024',
      '00000000-0000-0000-0000-000000000025',1,
      '00000000-0000-0000-0000-000000000001'
    );
    RAISE EXCEPTION 'stale travel should have failed';
  EXCEPTION WHEN serialization_failure THEN NULL;
  END;
END $$;

ROLLBACK;
