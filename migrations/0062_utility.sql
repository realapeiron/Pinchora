CREATE TABLE afk (
    guild_id BIGINT NOT NULL,
    user_id BIGINT NOT NULL,
    message TEXT NOT NULL CHECK (char_length(message) <= 100),
    since TIMESTAMPTZ NOT NULL DEFAULT now(),
    nick_prefixed BOOLEAN NOT NULL DEFAULT false,
    PRIMARY KEY (guild_id, user_id)
);
CREATE TABLE afk_reply_limits (
    channel_id BIGINT PRIMARY KEY,
    last_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE utility_settings (
    guild_id BIGINT PRIMARY KEY,
    timezone TEXT NOT NULL DEFAULT 'UTC'
);

CREATE TABLE sticky_messages (
    channel_id BIGINT PRIMARY KEY,
    guild_id BIGINT NOT NULL,
    content TEXT NOT NULL CHECK (char_length(content) <= 1000),
    embed JSONB,
    last_message_id BIGINT,
    min_gap_secs INTEGER NOT NULL DEFAULT 30 CHECK (min_gap_secs BETWEEN 5 AND 86400),
    min_messages INTEGER NOT NULL DEFAULT 5 CHECK (min_messages BETWEEN 1 AND 1000),
    message_count INTEGER NOT NULL DEFAULT 0,
    last_post_at TIMESTAMPTZ,
    enabled BOOLEAN NOT NULL DEFAULT true,
    updated_by BIGINT,
    lease_until TIMESTAMPTZ,
    pause_reason TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX sticky_messages_due ON sticky_messages (lease_until) WHERE enabled;
CREATE TABLE sticky_copy_cleanup (
    channel_id BIGINT NOT NULL,
    message_id BIGINT NOT NULL,
    PRIMARY KEY (channel_id, message_id)
);

CREATE TABLE scheduled_messages (
    id BIGSERIAL PRIMARY KEY,
    guild_id BIGINT NOT NULL,
    channel_id BIGINT NOT NULL,
    content TEXT NOT NULL CHECK (char_length(content) <= 2000),
    embed JSONB,
    schedule JSONB NOT NULL,
    tz TEXT NOT NULL DEFAULT 'UTC',
    next_run_at TIMESTAMPTZ NOT NULL,
    last_run_at TIMESTAMPTZ,
    runs INTEGER NOT NULL DEFAULT 0,
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_by BIGINT,
    ping_allowed BOOLEAN NOT NULL DEFAULT false,
    lease_until TIMESTAMPTZ,
    pause_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX scheduled_messages_due ON scheduled_messages (next_run_at) WHERE enabled;
CREATE INDEX scheduled_messages_guild ON scheduled_messages (guild_id);
