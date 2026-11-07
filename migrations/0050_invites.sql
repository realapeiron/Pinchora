CREATE TABLE invite_settings (
    guild_id BIGINT PRIMARY KEY REFERENCES guilds(id) ON DELETE CASCADE,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    log_channel_id BIGINT,
    fake_days INT NOT NULL DEFAULT 7 CHECK (fake_days BETWEEN 0 AND 365),
    temp_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    temp_channel_id BIGINT,
    temp_max_age_secs INT NOT NULL DEFAULT 172800 CHECK (temp_max_age_secs BETWEEN 60 AND 604800),
    temp_max_uses INT NOT NULL DEFAULT 1 CHECK (temp_max_uses BETWEEN 1 AND 100),
    temp_per_user_daily INT NOT NULL DEFAULT 3 CHECK (temp_per_user_daily BETWEEN 1 AND 20),
    temp_message_id BIGINT
);
CREATE TABLE invite_rewards (
    guild_id BIGINT NOT NULL REFERENCES guilds(id) ON DELETE CASCADE,
    invites INT NOT NULL CHECK (invites BETWEEN 1 AND 1000000),
    role_id BIGINT NOT NULL,
    PRIMARY KEY (guild_id, invites)
);
CREATE TABLE guild_invites (
    guild_id BIGINT NOT NULL REFERENCES guilds(id) ON DELETE CASCADE,
    code TEXT NOT NULL,
    inviter_id BIGINT,
    uses INT NOT NULL DEFAULT 0,
    max_uses INT NOT NULL DEFAULT 0,
    max_age INT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (guild_id, code)
);
CREATE TABLE invite_joins (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    guild_id BIGINT NOT NULL REFERENCES guilds(id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL,
    inviter_id BIGINT,
    code TEXT,
    joined_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    left_at TIMESTAMPTZ,
    fake BOOLEAN NOT NULL DEFAULT FALSE,
    source TEXT NOT NULL DEFAULT 'unknown' CHECK (source IN ('invite','vanity','unknown','temporary','widget'))
);
CREATE INDEX invite_joins_member ON invite_joins(guild_id, user_id, joined_at DESC);
CREATE INDEX invite_joins_inviter ON invite_joins(guild_id, inviter_id, joined_at DESC);
CREATE TABLE invite_bonus (
    guild_id BIGINT NOT NULL REFERENCES guilds(id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL,
    bonus INT NOT NULL DEFAULT 0,
    PRIMARY KEY (guild_id, user_id)
);

-- Keep reward authority and the roles the invite system has actually granted.
ALTER TABLE invite_rewards ADD COLUMN grantor_id BIGINT;
ALTER TABLE invite_settings ADD COLUMN baseline_required BOOLEAN NOT NULL DEFAULT FALSE;
CREATE TABLE invite_role_grants (
    guild_id BIGINT NOT NULL REFERENCES guilds(id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL,
    role_id BIGINT NOT NULL,
    PRIMARY KEY (guild_id, user_id, role_id)
);
