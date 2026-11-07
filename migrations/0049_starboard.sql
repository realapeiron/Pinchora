CREATE TABLE starboards (
    id BIGSERIAL PRIMARY KEY,
    guild_id BIGINT NOT NULL,
    name TEXT NOT NULL,
    channel_id BIGINT NOT NULL,
    emoji TEXT NOT NULL DEFAULT '⭐',
    threshold INT NOT NULL DEFAULT 3 CHECK (threshold BETWEEN 1 AND 50),
    self_star BOOLEAN NOT NULL DEFAULT FALSE,
    allow_bots BOOLEAN NOT NULL DEFAULT FALSE,
    only_channels BIGINT[] NOT NULL DEFAULT '{}',
    except_channels BIGINT[] NOT NULL DEFAULT '{}',
    require_image BOOLEAN NOT NULL DEFAULT FALSE,
    color INT NOT NULL DEFAULT 16766720,
    role_weights JSONB NOT NULL DEFAULT '{}'::jsonb,
    remove_below BOOLEAN NOT NULL DEFAULT TRUE,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    UNIQUE (guild_id, channel_id)
);
CREATE INDEX starboards_guild ON starboards (guild_id);
CREATE TABLE starboard_posts (
    starboard_id BIGINT NOT NULL REFERENCES starboards(id) ON DELETE CASCADE,
    message_id BIGINT NOT NULL,
    channel_id BIGINT NOT NULL,
    author_id BIGINT NOT NULL,
    star_count INT NOT NULL DEFAULT 0,
    board_message_id BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (starboard_id, message_id)
);
CREATE INDEX starboard_posts_top ON starboard_posts (starboard_id, star_count DESC, created_at DESC);
CREATE UNIQUE INDEX starboard_post_copy ON starboard_posts (board_message_id) WHERE board_message_id IS NOT NULL AND board_message_id > 0;
CREATE TABLE starboard_stars (
    starboard_id BIGINT NOT NULL,
    message_id BIGINT NOT NULL,
    user_id BIGINT NOT NULL,
    weight INT NOT NULL DEFAULT 1 CHECK (weight BETWEEN 1 AND 50),
    PRIMARY KEY (starboard_id, message_id, user_id),
    FOREIGN KEY (starboard_id, message_id) REFERENCES starboard_posts(starboard_id, message_id) ON DELETE CASCADE
);
CREATE TABLE vote_channels (
    channel_id BIGINT PRIMARY KEY,
    guild_id BIGINT NOT NULL,
    up_emoji TEXT NOT NULL DEFAULT '⬆️',
    down_emoji TEXT NOT NULL DEFAULT '⬇️',
    only_media BOOLEAN NOT NULL DEFAULT FALSE,
    create_thread BOOLEAN NOT NULL DEFAULT FALSE,
    enabled BOOLEAN NOT NULL DEFAULT TRUE
);
CREATE INDEX vote_channels_guild ON vote_channels (guild_id);

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
