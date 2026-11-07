CREATE TABLE starboard_deleted_sources (
    guild_id BIGINT NOT NULL,
    message_id BIGINT NOT NULL,
    PRIMARY KEY (guild_id, message_id)
);
CREATE TABLE starboard_copy_cleanup (
    channel_id BIGINT NOT NULL,
    copy_id BIGINT NOT NULL,
    due_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (channel_id, copy_id)
);
ALTER TABLE starboard_posts
    ADD COLUMN claim_token BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN sync_version BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN sync_due TIMESTAMPTZ,
    ADD COLUMN sync_token BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN checked_at TIMESTAMPTZ NOT NULL DEFAULT now();
CREATE INDEX starboard_posts_sync_due ON starboard_posts (sync_due) WHERE sync_due IS NOT NULL;
CREATE FUNCTION starboard_queue_removed_copies() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO starboard_copy_cleanup (channel_id, copy_id)
    SELECT OLD.channel_id, p.board_message_id FROM starboard_posts p
    WHERE p.starboard_id = OLD.id AND p.board_message_id > 0
    ON CONFLICT DO NOTHING;
    RETURN OLD;
END $$;
CREATE TRIGGER starboard_removed BEFORE DELETE ON starboards
    FOR EACH ROW EXECUTE FUNCTION starboard_queue_removed_copies();
