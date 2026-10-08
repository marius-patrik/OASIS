-- Atomic, idempotent ownership/custody moves. No engine-specific semantics.
BEGIN;

ALTER TABLE transactions ADD COLUMN request_digest TEXT;

CREATE OR REPLACE FUNCTION transfer_item(
    p_transaction_id UUID,
    p_idempotency_key TEXT,
    p_actor_entity_id UUID,
    p_item_entity_id UUID,
    p_expected_owner_id UUID,
    p_new_owner_id UUID,
    p_target_inventory_id UUID,
    p_target_presence_id UUID,
    p_expected_location_revision BIGINT,
    p_event_type_id UUID
)
RETURNS TABLE(result_transaction_id UUID, was_applied BOOLEAN)
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
DECLARE
    v_digest TEXT;
    v_tx transactions%ROWTYPE;
    v_actual_owner UUID;
    v_inventory_owner UUID;
    v_location_revision BIGINT;
BEGIN
    IF p_idempotency_key IS NULL OR length(p_idempotency_key) = 0 THEN
        RAISE EXCEPTION 'idempotency key is required' USING ERRCODE = '22023';
    END IF;
    IF (p_target_inventory_id IS NULL) = (p_target_presence_id IS NULL) THEN
        RAISE EXCEPTION 'exactly one destination is required' USING ERRCODE = '22023';
    END IF;
    IF p_actor_entity_id IS DISTINCT FROM p_expected_owner_id THEN
        RAISE EXCEPTION 'actor must own the item' USING ERRCODE = '42501';
    END IF;
    v_digest := md5(jsonb_build_array(
        p_actor_entity_id,p_item_entity_id,p_expected_owner_id,
        p_new_owner_id,p_target_inventory_id,p_target_presence_id,
        p_expected_location_revision,p_event_type_id
    )::text);

    -- The unique idempotency key makes concurrent identical requests wait for
    -- the winner; a committed retry returns the same transaction identifier.
    INSERT INTO transactions(id, actor_entity_id, idempotency_key, status, request_digest)
    VALUES(p_transaction_id, p_actor_entity_id, p_idempotency_key, 'pending', v_digest)
    ON CONFLICT(idempotency_key) DO NOTHING;

    SELECT * INTO v_tx FROM transactions t
      WHERE t.idempotency_key = p_idempotency_key FOR UPDATE;
    IF v_tx.request_digest IS DISTINCT FROM v_digest THEN
        RAISE EXCEPTION 'idempotency key reused for different operation'
          USING ERRCODE = '23505';
    END IF;
    IF v_tx.status = 'committed' THEN
        RETURN QUERY SELECT v_tx.id, FALSE;
        RETURN;
    END IF;
    IF v_tx.status <> 'pending' OR v_tx.id <> p_transaction_id THEN
        RAISE EXCEPTION 'transaction unavailable' USING ERRCODE = '40001';
    END IF;

    PERFORM 1 FROM items WHERE entity_id = p_item_entity_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'item not found' USING ERRCODE = '23503';
    END IF;
    SELECT o.owner_entity_id INTO v_actual_owner FROM ownerships o
        WHERE o.entity_id = p_item_entity_id FOR UPDATE;
    IF NOT FOUND OR v_actual_owner IS DISTINCT FROM p_expected_owner_id THEN
        RAISE EXCEPTION 'item ownership changed' USING ERRCODE = '40001';
    END IF;
    SELECT l.revision INTO v_location_revision FROM locations l
        WHERE l.entity_id = p_item_entity_id FOR UPDATE;
    IF NOT FOUND OR v_location_revision <> p_expected_location_revision THEN
        RAISE EXCEPTION 'stale item location revision' USING ERRCODE = '40001';
    END IF;
    IF p_target_inventory_id IS NOT NULL THEN
        SELECT i.owner_entity_id INTO v_inventory_owner FROM inventories i
            WHERE i.entity_id = p_target_inventory_id;
        IF NOT FOUND OR v_inventory_owner IS DISTINCT FROM p_new_owner_id THEN
            RAISE EXCEPTION 'destination inventory owner mismatch'
                USING ERRCODE = '23514';
        END IF;
    ELSE
        -- Foreign key on (presence_id,entity_id) makes ground presence
        -- refer to the actual item, never a different entity.
        PERFORM 1 FROM presences p
            WHERE p.id = p_target_presence_id
              AND p.entity_id = p_item_entity_id AND p.active;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'item target presence missing' USING ERRCODE = '23503';
        END IF;
    END IF;

    UPDATE ownerships SET owner_entity_id = p_new_owner_id, revision = revision + 1
       WHERE entity_id = p_item_entity_id;
    UPDATE locations SET container_entity_id = p_target_inventory_id,
       presence_id = p_target_presence_id, revision = revision + 1
       WHERE entity_id = p_item_entity_id;
    UPDATE transactions SET status = 'committed', committed_at = now()
       WHERE id = v_tx.id;
    INSERT INTO events(id, transaction_id, subject_entity_id, type_id, payload)
    VALUES (p_transaction_id, v_tx.id, p_item_entity_id, p_event_type_id,
            jsonb_build_object('from',p_expected_owner_id,'to',p_new_owner_id,
                'inventory',p_target_inventory_id,'presence',p_target_presence_id));
    RETURN QUERY SELECT v_tx.id, TRUE;
END;
$$;

COMMIT;
