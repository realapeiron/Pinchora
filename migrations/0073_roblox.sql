CREATE TABLE roblox_links (
    user_id BIGINT PRIMARY KEY,
    roblox_id BIGINT NOT NULL UNIQUE,
    roblox_name TEXT NOT NULL,
    verified_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    method TEXT NOT NULL DEFAULT 'profile_code' CHECK (method IN ('profile_code', 'oauth'))
);
CREATE TABLE roblox_pending (
    user_id BIGINT PRIMARY KEY,
    roblox_id BIGINT NOT NULL,
    roblox_name TEXT NOT NULL,
    code TEXT NOT NULL,
    last_check_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE roblox_settings (
    guild_id BIGINT PRIMARY KEY REFERENCES guilds(id) ON DELETE CASCADE,
    enabled BOOLEAN NOT NULL DEFAULT false,
    verified_role_id BIGINT,
    unverified_role_id BIGINT,
    nickname_template TEXT,
    verify_channel_id BIGINT,
    require_verified BOOLEAN NOT NULL DEFAULT false
);
CREATE TABLE roblox_binds (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    guild_id BIGINT NOT NULL REFERENCES guilds(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('verified', 'group_member', 'group_rank')),
    group_id BIGINT,
    rank_min SMALLINT,
    rank_max SMALLINT,
    role_id BIGINT NOT NULL,
    remove_if_unmet BOOLEAN NOT NULL DEFAULT true,
    CHECK ((kind = 'verified' AND group_id IS NULL AND rank_min IS NULL AND rank_max IS NULL)
        OR (kind = 'group_member' AND group_id > 0 AND rank_min IS NULL AND rank_max IS NULL)
        OR (kind = 'group_rank' AND group_id > 0 AND rank_min BETWEEN 1 AND 255
            AND rank_max BETWEEN rank_min AND 255))
);
CREATE INDEX roblox_binds_guild ON roblox_binds(guild_id);
CREATE TABLE roblox_cache (
    key TEXT PRIMARY KEY,
    payload JSONB NOT NULL,
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE roblox_sync (
    guild_id BIGINT NOT NULL REFERENCES guilds(id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL,
    next_update_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    held BOOLEAN NOT NULL DEFAULT false,
    PRIMARY KEY (guild_id, user_id)
);
CREATE INDEX roblox_sync_due ON roblox_sync(next_update_at);
