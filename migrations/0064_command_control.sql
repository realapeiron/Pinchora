-- D2: configuration only; no actor or member identifiers.
CREATE TABLE command_control (
    guild_id BIGINT PRIMARY KEY,
    config JSONB NOT NULL DEFAULT '{"rules":{},"aliases":{}}',
    revision BIGINT NOT NULL DEFAULT 0
);
CREATE TABLE config_restore_points (
    id BIGSERIAL PRIMARY KEY,
    guild_id BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    snapshot JSONB NOT NULL
);
CREATE INDEX config_restore_points_guild ON config_restore_points(guild_id, id DESC);
