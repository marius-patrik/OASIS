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
-- Second world and both execution contexts, with an active authoritative lease.
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
       '00000000-0000-0000-0000-000000000020',1,now()+INTERVAL '1 hour');
COMMIT;
