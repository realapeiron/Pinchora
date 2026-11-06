-- F1: AutoMod, bounded configuration and a 30-day, privacy-aware hit history.
CREATE TABLE automod_settings (
    guild_id BIGINT PRIMARY KEY,
    profile TEXT NOT NULL DEFAULT 'off' CHECK (profile IN ('off','relaxed','standard','strict')),
    escalation JSONB NOT NULL DEFAULT '[]',
    dm_on_action BOOLEAN NOT NULL DEFAULT FALSE,
    alert_channel_id BIGINT,
    observe_until TIMESTAMPTZ,
    revision BIGINT NOT NULL DEFAULT 0,
    sync_generation BIGINT NOT NULL DEFAULT 0,
    sync_after TIMESTAMPTZ,
    sync_error TEXT,
    synced_at TIMESTAMPTZ
);
CREATE TABLE automod_rules (
    id BIGSERIAL PRIMARY KEY,
    guild_id BIGINT NOT NULL REFERENCES automod_settings(guild_id) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 40),
    kind TEXT NOT NULL CHECK (kind IN ('spam','duplicate','repeat','bad_words','invites','links','caps','emoji','mentions','attachments','newlines','zalgo','media_only','youtube_only','honeypot','presets')),
    enabled BOOLEAN NOT NULL,
    log_only BOOLEAN NOT NULL,
    native BOOLEAN NOT NULL,
    discord_rule_id BIGINT,
    params JSONB NOT NULL,
    actions JSONB NOT NULL,
    exempt_roles BIGINT[] NOT NULL DEFAULT '{}',
    exempt_channels BIGINT[] NOT NULL DEFAULT '{}',
    only_channels BIGINT[] NOT NULL DEFAULT '{}',
    created_by BIGINT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(guild_id, name), UNIQUE(guild_id, discord_rule_id)
);
CREATE TABLE automod_strikes (
    guild_id BIGINT NOT NULL,
    user_id BIGINT NOT NULL,
    window_start TIMESTAMPTZ NOT NULL,
    count INTEGER NOT NULL CHECK (count > 0),
    PRIMARY KEY(guild_id, user_id)
);
CREATE TABLE automod_hits (
    id BIGSERIAL PRIMARY KEY,
    guild_id BIGINT NOT NULL,
    rule_id BIGINT,
    user_id BIGINT NOT NULL,
    channel_id BIGINT NOT NULL,
    message_id BIGINT,
    action TEXT NOT NULL,
    snippet TEXT CHECK (char_length(snippet) <= 200),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    reviewed BOOLEAN NOT NULL DEFAULT FALSE
);
CREATE INDEX automod_hits_recent ON automod_hits(guild_id, created_at DESC);
CREATE INDEX automod_hits_expiry ON automod_hits(created_at);
CREATE UNIQUE INDEX automod_hits_once ON automod_hits(guild_id, rule_id, message_id) WHERE message_id IS NOT NULL;
CREATE TABLE automod_hit_actions (
    hit_id BIGINT NOT NULL REFERENCES automod_hits(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('delete','timeout','ban')),
    claimed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (hit_id, kind)
);
