-- F10: durable gate, bounded recent joins, and reversible raid state.
CREATE TABLE verification (
 guild_id BIGINT PRIMARY KEY,
 enabled BOOLEAN NOT NULL DEFAULT false, mode TEXT NOT NULL DEFAULT 'captcha',
 verified_role_id TEXT, unverified_role_id TEXT, channel_id TEXT, message_id TEXT,
 kick_after_mins INTEGER NOT NULL DEFAULT 0, min_account_age_hours INTEGER NOT NULL DEFAULT 0,
 block_default_avatar BOOLEAN NOT NULL DEFAULT false, block_name_patterns BOOLEAN NOT NULL DEFAULT false,
 dm_on_block BOOLEAN NOT NULL DEFAULT false, log_channel_id TEXT, message TEXT NOT NULL DEFAULT '',
 rule_action TEXT NOT NULL DEFAULT 'hold', exempt_roles TEXT[] NOT NULL DEFAULT '{}', exempt_users BIGINT[] NOT NULL DEFAULT '{}'
);
CREATE TABLE verification_pending (
 guild_id BIGINT NOT NULL, user_id BIGINT NOT NULL, joined_at TIMESTAMPTZ NOT NULL,
 attempts INTEGER NOT NULL DEFAULT 0, expires_at TIMESTAMPTZ, verified_at TIMESTAMPTZ,
 PRIMARY KEY (guild_id,user_id)
);
CREATE INDEX verification_due ON verification_pending(expires_at) WHERE verified_at IS NULL;
CREATE TABLE raid_settings (
 guild_id BIGINT PRIMARY KEY, enabled BOOLEAN NOT NULL DEFAULT false, join_threshold INTEGER NOT NULL DEFAULT 10,
 window_secs INTEGER NOT NULL DEFAULT 30, action TEXT NOT NULL DEFAULT 'alert', lockdown_minutes INTEGER NOT NULL DEFAULT 10,
 alert_channel_id TEXT, similar_threshold INTEGER NOT NULL DEFAULT 5, similar_names BOOLEAN NOT NULL DEFAULT true,
 similar_avatars BOOLEAN NOT NULL DEFAULT true, new_account_hours INTEGER NOT NULL DEFAULT 24,
 channels TEXT[] NOT NULL DEFAULT '{}', slowmode_secs INTEGER NOT NULL DEFAULT 10,
 lock_channels BOOLEAN NOT NULL DEFAULT false, pause_invites BOOLEAN NOT NULL DEFAULT false
);
CREATE TABLE raid_events (
 id BIGSERIAL PRIMARY KEY, guild_id BIGINT NOT NULL, started_at TIMESTAMPTZ NOT NULL, ended_at TIMESTAMPTZ,
 joins INTEGER NOT NULL, action TEXT NOT NULL, ended_by BIGINT, expires_at TIMESTAMPTZ NOT NULL,
 snapshot JSONB NOT NULL DEFAULT '{}', error TEXT
);
CREATE UNIQUE INDEX raid_active ON raid_events(guild_id) WHERE ended_at IS NULL;
CREATE TABLE raid_joins (
 guild_id BIGINT NOT NULL, user_id BIGINT NOT NULL, joined_at TIMESTAMPTZ NOT NULL,
 name_family TEXT NOT NULL, avatar TEXT, young BOOLEAN NOT NULL, status TEXT NOT NULL DEFAULT 'joined',
 PRIMARY KEY (guild_id,user_id)
);
CREATE INDEX raid_joins_window ON raid_joins(guild_id,joined_at);
