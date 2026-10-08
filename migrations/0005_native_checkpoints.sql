-- OASIS persistent native state is committed in the SAME transaction as
-- presence and authority handoff. This is a universal operation: no native
-- engine or game name appears anywhere in the database contract.
BEGIN;

-- Every stored revision of the same entity is unique, preserving a complete
-- history of native snapshots across world instances.
CREATE UNIQUE INDEX state_documents_entity_revision
    ON state_documents(entity_id, revision) WHERE entity_id IS NOT NULL;

CREATE TABLE travel_checkpoints (
    transaction_id UUID PRIMARY KEY REFERENCES transactions(id),
    state_document_id UUID NOT NULL UNIQUE REFERENCES state_documents(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE OR REPLACE FUNCTION persist_character_travel(
    p_transaction_id UUID,
    p_idempotency_key TEXT,
    p_session_id UUID,
    p_character_id UUID,
    p_expected_presence_id UUID,
    p_new_presence_id UUID,
    p_destination_instance_id UUID,
    p_destination_frame_id UUID,
    p_destination_context_id UUID,
    p_expected_authority_epoch BIGINT,
    p_event_type_id UUID,
    p_snapshot_id UUID,
    p_snapshot_type_id UUID,
    p_expected_snapshot_revision BIGINT,
    p_snapshot JSONB
)
RETURNS TABLE(
    result_transaction_id UUID,
    authority_epoch BIGINT,
    was_applied BOOLEAN,
    snapshot_revision BIGINT
)
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
DECLARE
    v_result RECORD;
    v_current_revision BIGINT;
    v_committed_revision BIGINT;
BEGIN
    IF p_expected_snapshot_revision < 0 OR p_snapshot IS NULL
       OR jsonb_typeof(p_snapshot) IS DISTINCT FROM 'object' THEN
        RAISE EXCEPTION 'native snapshot must be an object with nonnegative revision'
            USING ERRCODE = '22023';
    END IF;

    -- Invoked in the caller's SQL transaction. The lower-level routine locks
    -- the character's entity row, validates the authenticated session, fences
    -- stale authority and commits its new presence and event *as part of this
    -- very same transaction*. A later exception rolls everything back.
    SELECT * INTO STRICT v_result FROM transfer_character_presence(
        p_transaction_id, p_idempotency_key, p_session_id, p_character_id,
        p_expected_presence_id, p_new_presence_id, p_destination_instance_id,
        p_destination_frame_id, p_destination_context_id,
        p_expected_authority_epoch, p_event_type_id
    );

    IF NOT v_result.was_applied THEN
        SELECT doc.revision INTO v_committed_revision
          FROM travel_checkpoints ck
          JOIN state_documents doc ON doc.id = ck.state_document_id
         WHERE ck.transaction_id = v_result.result_transaction_id
           AND doc.entity_id = p_character_id;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'existing transfer lacks a checkpoint'
                USING ERRCODE = '40001';
        END IF;
        RETURN QUERY SELECT v_result.result_transaction_id,
                            v_result.new_authority_epoch,
                            FALSE, v_committed_revision;
        RETURN;
    END IF;

    SELECT max(doc.revision) INTO v_current_revision
      FROM state_documents doc WHERE doc.entity_id = p_character_id;
    IF COALESCE(v_current_revision,0) <> p_expected_snapshot_revision THEN
        RAISE EXCEPTION 'stale native snapshot revision'
            USING ERRCODE = '40001';
    END IF;

    INSERT INTO state_documents(
        id, type_id, entity_id, data, revision
    ) VALUES (
        p_snapshot_id, p_snapshot_type_id, p_character_id,
        p_snapshot, p_expected_snapshot_revision + 1
    );
    INSERT INTO travel_checkpoints(transaction_id, state_document_id)
    VALUES(v_result.result_transaction_id, p_snapshot_id);

    RETURN QUERY SELECT v_result.result_transaction_id,
                        v_result.new_authority_epoch, TRUE,
                        p_expected_snapshot_revision + 1;
END;
$$;

COMMIT;
