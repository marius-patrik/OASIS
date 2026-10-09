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
        '{"k":"map","v":{"gameSpecific":{"k":"sequence","v":[{"k":"int","v":1},{"k":"int","v":2},{"k":"int","v":3}]},"arbitrary":{"k":"map","v":{"nesting":{"k":"int","v":5}}},"bytecode":{"k":"bytes","v":[0,1,254,255]},"maxUnsigned":{"k":"uint","v":18446744073709551615},"nativeRef":{"k":"ref","v":"340282366920938463463374607431768211455"}}}');

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


-- Checkpoint and travel are one atomic operation: native state is preserved
-- without replacing the identity or engine module behind the character.
DO $$
DECLARE
  result_epoch BIGINT;
  persisted_rev BIGINT;
  applied BOOLEAN;
BEGIN
  SELECT authority_epoch, snapshot_revision, was_applied
    INTO result_epoch, persisted_rev, applied
    FROM persist_character_travel(
      '00000000-0000-0000-0000-000000000040','snapshot-travel-1',
      '00000000-0000-0000-0000-000000000021',
      '00000000-0000-0000-0000-000000000007',
      '00000000-0000-0000-0000-000000000029',
      '00000000-0000-0000-0000-000000000041',
      '00000000-0000-0000-0000-000000000023',
      '00000000-0000-0000-0000-000000000024',
      '00000000-0000-0000-0000-000000000025',3,
      '00000000-0000-0000-0000-000000000001',
      '00000000-0000-0000-0000-000000000042',
      '00000000-0000-0000-0000-000000000001',0,
      '{"native":{"module":"fixture","value":17},"revision":7}'::jsonb
    );
  IF result_epoch <> 4 OR persisted_rev <> 1 OR NOT applied THEN
    RAISE EXCEPTION 'native checkpoint transfer failed';
  END IF;

  -- The same request can be retried after newer native state exists: its
  -- original epoch and version are recovered from the committed checkpoint.
  SELECT authority_epoch, snapshot_revision, was_applied
    INTO result_epoch, persisted_rev, applied
    FROM persist_character_travel(
      '00000000-0000-0000-0000-000000000040','snapshot-travel-1',
      '00000000-0000-0000-0000-000000000021',
      '00000000-0000-0000-0000-000000000007',
      '00000000-0000-0000-0000-000000000029',
      '00000000-0000-0000-0000-000000000041',
      '00000000-0000-0000-0000-000000000023',
      '00000000-0000-0000-0000-000000000024',
      '00000000-0000-0000-0000-000000000025',3,
      '00000000-0000-0000-0000-000000000001',
      '00000000-0000-0000-0000-000000000042',
      '00000000-0000-0000-0000-000000000001',0,
      '{"native":{"module":"fixture","value":17},"revision":7}'::jsonb
    );
  IF result_epoch <> 4 OR persisted_rev <> 1 OR applied THEN
    RAISE EXCEPTION 'checkpoint retry was not idempotent';
  END IF;

  BEGIN
    PERFORM * FROM persist_character_travel(
      '00000000-0000-0000-0000-000000000040','snapshot-travel-1',
      '00000000-0000-0000-0000-000000000021',
      '00000000-0000-0000-0000-000000000007',
      '00000000-0000-0000-0000-000000000029',
      '00000000-0000-0000-0000-000000000041',
      '00000000-0000-0000-0000-000000000023',
      '00000000-0000-0000-0000-000000000024',
      '00000000-0000-0000-0000-000000000025',3,
      '00000000-0000-0000-0000-000000000001',
      '00000000-0000-0000-0000-000000000042',
      '00000000-0000-0000-0000-000000000001',0,
      '{"native":{"module":"fixture","value":999},"revision":7}'::jsonb
    );
    RAISE EXCEPTION 'changed idempotent checkpoint was accepted';
  EXCEPTION WHEN unique_violation THEN NULL;
  END;
  BEGIN
    -- An incorrect version must reject the WHOLE transfer, including new
    -- presence and authority. A savepoint is implicit in this PL/pgSQL block.
    PERFORM * FROM persist_character_travel(
      '00000000-0000-0000-0000-000000000043','snapshot-travel-stale',
      '00000000-0000-0000-0000-000000000021',
      '00000000-0000-0000-0000-000000000007',
      '00000000-0000-0000-0000-000000000041',
      '00000000-0000-0000-0000-000000000044',
      '00000000-0000-0000-0000-000000000013',
      '00000000-0000-0000-0000-000000000014',
      '00000000-0000-0000-0000-000000000020',4,
      '00000000-0000-0000-0000-000000000001',
      '00000000-0000-0000-0000-000000000045',
      '00000000-0000-0000-0000-000000000001',0,
      '{"native":{"value":999}}'::jsonb
    );
    RAISE EXCEPTION 'incorrect version must fail';
  EXCEPTION WHEN serialization_failure THEN NULL;
  END;
  IF (SELECT count(*) FROM presences p WHERE p.entity_id =
      '00000000-0000-0000-0000-000000000007' AND p.active) <> 1 THEN
    RAISE EXCEPTION 'stale checkpoint left duplicate presence';
  END IF;
  IF (SELECT epoch FROM authority_leases WHERE resource_key =
      'character:00000000-0000-0000-0000-000000000007') <> 4 THEN
    RAISE EXCEPTION 'stale checkpoint modified authority';
  END IF;
  IF (SELECT doc.data #>> '{native,value}' FROM state_documents doc
      WHERE doc.id='00000000-0000-0000-0000-000000000042') <> '17' THEN
    RAISE EXCEPTION 'native game state lost';
  END IF;
  IF (SELECT count(*) FROM travel_checkpoints) <> 1 THEN
    RAISE EXCEPTION 'travel checkpoint duplicated';
  END IF;
END $$;


-- Recovery uses the same character, presence and originating native module.
INSERT INTO execution_contexts(id,world_instance_id,module_id,status)
VALUES('00000000-0000-0000-0000-000000000050',
       '00000000-0000-0000-0000-000000000023',
       '00000000-0000-0000-0000-000000000005','running');
DO $$
DECLARE
  recovered BIGINT;
BEGIN
  SELECT renew_character_authority(
    '00000000-0000-0000-0000-000000000021',
    '00000000-0000-0000-0000-000000000007',
    '00000000-0000-0000-0000-000000000025',4
  ) INTO recovered;
  IF recovered <> 4 THEN RAISE EXCEPTION 'renewal changed authority epoch'; END IF;
  BEGIN
    PERFORM reclaim_character_authority(
      '00000000-0000-0000-0000-000000000021',
      '00000000-0000-0000-0000-000000000007',
      '00000000-0000-0000-0000-000000000050',4
    );
    RAISE EXCEPTION 'live authority was stolen';
  EXCEPTION WHEN serialization_failure THEN NULL;
  END;
  UPDATE authority_leases SET expires_at=now()-INTERVAL '1 second'
    WHERE resource_key='character:00000000-0000-0000-0000-000000000007';
  BEGIN
    PERFORM reclaim_character_authority(
      '00000000-0000-0000-0000-000000000999',
      '00000000-0000-0000-0000-000000000007',
      '00000000-0000-0000-0000-000000000050',4
    );
    RAISE EXCEPTION 'unauthorized takeover succeeded';
  EXCEPTION WHEN insufficient_privilege THEN NULL;
  END;
  SELECT reclaim_character_authority(
    '00000000-0000-0000-0000-000000000021',
    '00000000-0000-0000-0000-000000000007',
    '00000000-0000-0000-0000-000000000050',4
  ) INTO recovered;
  IF recovered <> 5 THEN RAISE EXCEPTION 'recovery epoch did not advance'; END IF;
  IF (SELECT holder_context_id FROM authority_leases WHERE resource_key=
      'character:00000000-0000-0000-0000-000000000007')
      <> '00000000-0000-0000-0000-000000000050'::uuid THEN
    RAISE EXCEPTION 'authority not reassigned to new native context';
  END IF;
  BEGIN
    PERFORM renew_character_authority(
      '00000000-0000-0000-0000-000000000021',
      '00000000-0000-0000-0000-000000000007',
      '00000000-0000-0000-0000-000000000025',4
    );
    RAISE EXCEPTION 'old context renewed after fencing';
  EXCEPTION WHEN serialization_failure THEN NULL;
  END;
  BEGIN
    PERFORM reclaim_character_authority(
      '00000000-0000-0000-0000-000000000021',
      '00000000-0000-0000-0000-000000000007',
      '00000000-0000-0000-0000-000000000050',4
    );
    RAISE EXCEPTION 'stale recovery was accepted';
  EXCEPTION WHEN serialization_failure THEN NULL;
  END;
  IF (SELECT count(*) FROM presences WHERE entity_id=
      '00000000-0000-0000-0000-000000000007' AND active) <> 1 THEN
    RAISE EXCEPTION 'recovery changed identity or world presence';
  END IF;
  IF (SELECT origin_module_id FROM entities WHERE id=
      '00000000-0000-0000-0000-000000000007')
      <> '00000000-0000-0000-0000-000000000005'::uuid THEN
    RAISE EXCEPTION 'recovery changed originating game engine';
  END IF;
END $$;


-- Canonical typed components must be unambiguous to every native adapter.
DO $$
BEGIN
  BEGIN
    INSERT INTO components(id,entity_id,type_id,slot,data)
    VALUES('00000000-0000-0000-0000-000000000098',
           '00000000-0000-0000-0000-000000000007',
           '00000000-0000-0000-0000-000000000001',
           'invalid-plain-json', '{}'::jsonb);
    RAISE EXCEPTION 'untagged native component was accepted';
  EXCEPTION WHEN check_violation THEN NULL;
  END;

  INSERT INTO components(id,entity_id,type_id,slot)
  VALUES('00000000-0000-0000-0000-000000000099',
         '00000000-0000-0000-0000-000000000007',
         '00000000-0000-0000-0000-000000000001',
         'native-default');
  IF (SELECT data FROM components
      WHERE id='00000000-0000-0000-0000-000000000099')
      IS DISTINCT FROM '{"k":"map","v":{}}'::jsonb THEN
    RAISE EXCEPTION 'native component default lost its type';
  END IF;
END $$;

ROLLBACK;
