-- Fenced server recovery: live contexts renew a lease, expired contexts
-- can be reclaimed by the same player in the same world and source module.
-- No game-specific module behavior or pairwise mappings.
BEGIN;

CREATE OR REPLACE FUNCTION renew_character_authority(
    p_session_id UUID,
    p_character_id UUID,
    p_context_id UUID,
    p_expected_epoch BIGINT
)
RETURNS BIGINT
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
DECLARE
    v_epoch BIGINT;
BEGIN
    PERFORM 1 FROM sessions s
      WHERE s.id=p_session_id AND s.active_character_id=p_character_id
        AND s.status='active' AND s.expires_at>now();
    IF NOT FOUND THEN
        RAISE EXCEPTION 'session not authorized to renew character lease'
            USING ERRCODE='42501';
    END IF;

    UPDATE authority_leases a
       SET expires_at=clock_timestamp()+INTERVAL '60 seconds'
     WHERE a.resource_key='character:'||p_character_id::text
       AND a.holder_context_id=p_context_id
       AND a.epoch=p_expected_epoch
       AND a.expires_at>clock_timestamp()
    RETURNING a.epoch INTO v_epoch;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'lease is expired, stale or belongs to another context'
            USING ERRCODE='40001';
    END IF;
    RETURN v_epoch;
END;
$$;

CREATE OR REPLACE FUNCTION reclaim_character_authority(
    p_session_id UUID,
    p_character_id UUID,
    p_new_context_id UUID,
    p_expected_epoch BIGINT
)
RETURNS BIGINT
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
DECLARE
    v_world UUID;
    v_epoch BIGINT;
    v_expires_at TIMESTAMPTZ;
BEGIN
    -- Serialize recovery with travel, so the world and module identity
    -- checked below cannot be changed underneath this transaction.
    PERFORM 1 FROM entities e WHERE e.id=p_character_id FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'entity missing' USING ERRCODE='23503';
    END IF;
    PERFORM 1 FROM sessions s
      WHERE s.id=p_session_id AND s.active_character_id=p_character_id
        AND s.status='active' AND s.expires_at>now();
    IF NOT FOUND THEN
        RAISE EXCEPTION 'session not authorized to recover character'
            USING ERRCODE='42501';
    END IF;
    SELECT p.world_instance_id INTO v_world FROM presences p
      WHERE p.entity_id=p_character_id AND p.active FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'character has no active world presence'
            USING ERRCODE='23503';
    END IF;

    -- A recovered character is allowed to continue only through its original
    -- module. It must not switch engines or world locations during recovery.
    PERFORM 1 FROM execution_contexts c
      JOIN entities e ON e.id=p_character_id
      JOIN world_instances w ON w.id=c.world_instance_id
     WHERE c.id=p_new_context_id AND c.status='running'
       AND c.world_instance_id=v_world AND w.status='running'
       AND c.module_id=e.origin_module_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'new execution context is not the native owner in current world'
            USING ERRCODE='23514';
    END IF;

    SELECT a.epoch,a.expires_at INTO v_epoch,v_expires_at
      FROM authority_leases a
     WHERE a.resource_key='character:'||p_character_id::text FOR UPDATE;
    IF NOT FOUND OR v_epoch <> p_expected_epoch THEN
        RAISE EXCEPTION 'stale authority epoch' USING ERRCODE='40001';
    END IF;
    IF v_expires_at>clock_timestamp() THEN
        RAISE EXCEPTION 'cannot reclaim live authority' USING ERRCODE='40001';
    END IF;

    UPDATE authority_leases
       SET holder_context_id=p_new_context_id,
           epoch=epoch+1,
           expires_at=clock_timestamp()+INTERVAL '60 seconds'
     WHERE resource_key='character:'||p_character_id::text
    RETURNING epoch INTO v_epoch;
    RETURN v_epoch;
END;
$$;

COMMIT;
