-- Database fencing survives a Redis restart, including loss of all ephemeral keys.
CREATE SEQUENCE reef_fencing_token;
CREATE TABLE reef_room_leases (
    namespace TEXT NOT NULL,
    owner_id BIGINT NOT NULL,
    room_idx SMALLINT NOT NULL,
    token BIGINT NOT NULL DEFAULT nextval('reef_fencing_token'),
    expires_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY(namespace, owner_id, room_idx)
);

-- One durable notification per house per transaction, including multi-row restore/clear.
-- Triggers cover all bot/web mutation paths, including direct updates and cascades.
CREATE TABLE reef_layout_outbox (
    id BIGSERIAL PRIMARY KEY,
    owner_id BIGINT NOT NULL REFERENCES houses(user_id) ON DELETE CASCADE,
    transaction_id BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    UNIQUE(owner_id, transaction_id)
);
CREATE INDEX reef_layout_outbox_retention ON reef_layout_outbox(created_at);
CREATE FUNCTION reef_layout_changed() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE house_owner BIGINT;
BEGIN
    IF TG_OP = 'DELETE' THEN house_owner := OLD.user_id;
    ELSE house_owner := NEW.user_id; END IF;
    IF EXISTS(SELECT 1 FROM houses WHERE user_id=house_owner) THEN
        INSERT INTO reef_layout_outbox(owner_id,transaction_id)
        VALUES(house_owner,txid_current()) ON CONFLICT DO NOTHING;
    END IF;
    RETURN NULL;
END $$;
CREATE TRIGGER reef_placements_outbox AFTER INSERT OR UPDATE OR DELETE ON placements
    FOR EACH ROW EXECUTE FUNCTION reef_layout_changed();
CREATE TRIGGER reef_rooms_outbox AFTER INSERT OR UPDATE OR DELETE ON rooms
    FOR EACH ROW EXECUTE FUNCTION reef_layout_changed();
CREATE TRIGGER reef_houses_outbox AFTER UPDATE OF tier ON houses
    FOR EACH ROW WHEN (OLD.tier IS DISTINCT FROM NEW.tier) EXECUTE FUNCTION reef_layout_changed();
