-- Performance indexes and structural uniqueness not provided by plain FKs.
BEGIN;
CREATE UNIQUE INDEX players_handle_casefold_uq ON players(lower(handle));
CREATE UNIQUE INDEX presences_one_active_per_entity_uq ON presences(entity_id) WHERE active;
CREATE INDEX entities_definition_idx ON entities(definition_id);
CREATE INDEX entity_definitions_type_idx ON entity_definitions(type_id);
CREATE INDEX characters_player_idx ON characters(player_id);
CREATE INDEX engine_modules_engine_idx ON engine_modules(engine_id);
CREATE INDEX worlds_game_idx ON worlds(game_id);
CREATE INDEX world_instances_world_status_idx ON world_instances(world_id,status);
CREATE INDEX presences_world_active_idx ON presences(world_instance_id,entity_id) WHERE active;
CREATE INDEX components_type_idx ON components(type_id,entity_id);
CREATE INDEX capabilities_module_idx ON capabilities(module_id);
CREATE INDEX relations_out_idx ON relations(source_entity_id,type_id);
CREATE INDEX relations_in_idx ON relations(target_entity_id,type_id);
CREATE INDEX ownerships_owner_idx ON ownerships(owner_entity_id);
CREATE INDEX locations_container_idx ON locations(container_entity_id);
CREATE INDEX locations_presence_idx ON locations(presence_id);
CREATE INDEX engine_bindings_module_idx ON engine_bindings(module_id);
CREATE INDEX execution_contexts_world_idx ON execution_contexts(world_instance_id);
CREATE INDEX authority_leases_holder_idx ON authority_leases(holder_context_id);
CREATE INDEX state_documents_entity_idx ON state_documents(entity_id,type_id,revision DESC) WHERE entity_id IS NOT NULL;
CREATE INDEX state_documents_world_idx ON state_documents(world_instance_id,type_id,revision DESC) WHERE world_instance_id IS NOT NULL;
CREATE INDEX events_subject_seq_idx ON events(subject_entity_id,sequence);
CREATE INDEX events_transaction_idx ON events(transaction_id);
-- JSONB indexes should be targeted to fields declared/queryable in type schemas;
-- do not automatically GIN-index every arbitrary payload.
COMMIT;