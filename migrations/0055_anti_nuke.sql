CREATE TABLE antinuke (
    guild_id BIGINT PRIMARY KEY,
    enabled BOOLEAN NOT NULL DEFAULT FALSE,
    limits JSONB NOT NULL DEFAULT '{}',
    response TEXT NOT NULL DEFAULT 'alert' CHECK (response IN ('alert','quarantine','kick','ban')),
    log_channel_id BIGINT,
    quarantine_role_id BIGINT,
    vip_roles BIGINT[] NOT NULL DEFAULT '{}',
    trusted_users BIGINT[] NOT NULL DEFAULT '{}',
    owner_only_config BOOLEAN NOT NULL DEFAULT TRUE,
    auto_restore BOOLEAN NOT NULL DEFAULT FALSE,
    revert_vip_bans BOOLEAN NOT NULL DEFAULT FALSE,
    remove_bots BOOLEAN NOT NULL DEFAULT FALSE,
    CHECK (cardinality(vip_roles)<=50 AND cardinality(trusted_users)<=50)
);
CREATE TABLE antinuke_events (
    id BIGINT PRIMARY KEY,
    guild_id BIGINT NOT NULL,
    actor_id BIGINT NOT NULL,
    target_user_id BIGINT,
    target_id BIGINT,
    kind TEXT NOT NULL,
    count INTEGER NOT NULL DEFAULT 0,
    action_taken TEXT NOT NULL DEFAULT 'observed',
    details JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    reverted BOOLEAN NOT NULL DEFAULT FALSE,
    recovery_state TEXT NOT NULL DEFAULT 'ready'
);
CREATE INDEX antinuke_events_actor ON antinuke_events(guild_id,actor_id,created_at DESC);
CREATE TABLE antinuke_role_backups (
    guild_id BIGINT NOT NULL,
    user_id BIGINT NOT NULL,
    role_ids BIGINT[] NOT NULL CHECK(cardinality(role_ids)<=250),
    taken_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    quarantined BOOLEAN NOT NULL DEFAULT FALSE,
    restore_on_join BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY(guild_id,user_id)
);
CREATE TABLE antinuke_snapshots (
    guild_id BIGINT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('channel','role')),
    object_id BIGINT NOT NULL,
    data JSONB NOT NULL CHECK(octet_length(data::text)<=65536),
    taken_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    restored_id BIGINT,
    PRIMARY KEY(guild_id,kind,object_id)
);
-- User overwrites are normalized so privacy deletion cannot leave their IDs inside snapshot JSON.
CREATE TABLE antinuke_overwrites (
    guild_id BIGINT NOT NULL,
    channel_id BIGINT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'channel' CHECK(kind='channel'),
    user_id BIGINT NOT NULL,
    allow_bits TEXT NOT NULL,
    deny_bits TEXT NOT NULL,
    PRIMARY KEY(guild_id,channel_id,user_id),
    FOREIGN KEY(guild_id,kind,channel_id) REFERENCES antinuke_snapshots(guild_id,kind,object_id) ON DELETE CASCADE
);
