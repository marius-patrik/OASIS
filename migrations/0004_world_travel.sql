-- Durable authoritative character travel. Each invocation is one SQL
-- transaction, and both presence and server authority are fenced in one write.
-- No game- or engine-pair-specific translations.
BEGIN;

CREATE OR REPLACE FUNCTION transfer_character_presence(
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
    p_event_type_id UUID
)
RETURNS TABLE(result_transaction_id UUID, new_authority_epoch BIGINT, was_applied BOOLEAN)
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
DECLARE
    v_digest TEXT;
    v_tx transactions%ROWTYPE;
    v_current_id UUID;
    v_current_epoch BIGINT;
    v_origin_world UUID;
    v_destination_world UUID;
    v_key TEXT;
BEGIN
    IF p_idempotency_key IS NULL OR length(p_idempotency_key) = 0 THEN
        RAISE EXCEPTION 'idempotency key is required' USING ERRCODE='22023';
    END IF;
    IF p_expected_authority_epoch < 0 THEN
        RAISE EXCEPTION 'invalid expected authority epoch' USING ERRCODE='22023';
    END IF;
    v_digest := md5(jsonb_build_array(
        p_session_id,p_character_id,p_expected_presence_id,
        p_new_presence_id,p_destination_instance_id,p_destination_frame_id,
        p_destination_context_id,p_expected_authority_epoch,p_event_type_id
    )::text);
    INSERT INTO transactions(id, actor_entity_id, idempotency_key, status, request_digest)
    VALUES(p_transaction_id, p_character_id, p_idempotency_key, 'pending', v_digest)
    ON CONFLICT(idempotency_key) DO NOTHING;

    SELECT * INTO v_tx FROM transactions t
       WHERE t.idempotency_key=p_idempotency_key FOR UPDATE;
    IF v_tx.request_digest IS DISTINCT FROM v_digest THEN
        RAISE EXCEPTION 'reused idempotency key has different payload'
          USING ERRCODE='23505';
    END IF;
    IF v_tx.status = 'committed' THEN
        SELECT a.epoch INTO v_current_epoch FROM authority_leases a
            WHERE a.resource_key = ('character:' || p_character_id::text);
        RETURN QUERY SELECT v_tx.id, v_current_epoch, FALSE;
        RETURN;
    END IF;
    IF v_tx.status <> 'pending' OR v_tx.id <> p_transaction_id THEN
        RAISE EXCEPTION 'transaction unavailable' USING ERRCODE='40001';
    END IF;

    PERFORM 1 FROM sessions s
      WHERE s.id=p_session_id AND s.active_character_id=p_character_id
        AND s.status='active' AND s.expires_at > now() FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'session cannot control this character'
          USING ERRCODE='42501';
    END IF;

    SELECT w.world_id INTO v_destination_world FROM world_instances w
        WHERE w.id=p_destination_instance_id AND w.status='running';
    IF NOT FOUND THEN
        RAISE EXCEPTION 'destination world is not running'
          USING ERRCODE='23503';
    END IF;
    PERFORM 1 FROM spatial_frames f
       WHERE f.id=p_destination_frame_id AND f.world_id=v_destination_world;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'destination frame does not belong to world'
          USING ERRCODE='23514';
    END IF;
    PERFORM 1 FROM execution_contexts c
       WHERE c.id=p_destination_context_id AND
         c.world_instance_id=p_destination_instance_id AND c.status='running';
    IF NOT FOUND THEN
        RAISE EXCEPTION 'destination context is not running in destination'
          USING ERRCODE='23514';
    END IF;

    -- This row is the character's serialization point for all world travel.
    PERFORM 1 FROM entities e WHERE e.id=p_character_id FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'character entity missing' USING ERRCODE='23503';
    END IF;
    SELECT p.id INTO v_current_id FROM presences p
        WHERE p.entity_id=p_character_id AND p.active FOR UPDATE;
    IF v_current_id IS DISTINCT FROM p_expected_presence_id THEN
        RAISE EXCEPTION 'stale source presence' USING ERRCODE='40001';
    END IF;

    v_key := 'character:' || p_character_id::text;
    SELECT a.epoch INTO v_current_epoch FROM authority_leases a
        WHERE a.resource_key=v_key FOR UPDATE;
    IF COALESCE(v_current_epoch, 0) <> p_expected_authority_epoch THEN
        RAISE EXCEPTION 'stale source authority' USING ERRCODE='40001';
    END IF;

    IF v_current_id IS NOT NULL THEN
        SELECT p.world_instance_id INTO v_origin_world FROM presences p
          WHERE p.id=v_current_id;
        UPDATE presences SET active=FALSE, ended_at=now()
          WHERE id=v_current_id AND active;
    END IF;

    INSERT INTO presences(id,entity_id,world_instance_id,frame_id,active)
    VALUES(p_new_presence_id,p_character_id,p_destination_instance_id,
           p_destination_frame_id,TRUE);

    IF v_current_epoch IS NULL THEN
        INSERT INTO authority_leases(resource_key,holder_context_id,epoch,expires_at)
        VALUES(v_key,p_destination_context_id,1,now()+INTERVAL '60 seconds');
        v_current_epoch := 1;
    ELSE
        UPDATE authority_leases SET holder_context_id=p_destination_context_id,
            epoch=epoch+1,expires_at=now()+INTERVAL '60 seconds'
          WHERE resource_key=v_key;
        v_current_epoch := v_current_epoch+1;
    END IF;

    UPDATE transactions SET status='committed',committed_at=now()
      WHERE id=v_tx.id;
    INSERT INTO events(id,transaction_id,subject_entity_id,type_id,payload)
    VALUES(p_transaction_id,v_tx.id,p_character_id,p_event_type_id,
      jsonb_build_object(
        'previous_presence',v_current_id,
        'new_presence',p_new_presence_id,
        'previous_world',v_origin_world,
        'new_world',p_destination_instance_id,
        'authority_epoch',v_current_epoch
      ));
    RETURN QUERY SELECT v_tx.id, v_current_epoch, TRUE;
END;
$$;

COMMIT;
