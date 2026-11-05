-- Discord snowflakes fit in BIGINT (they stay below 2^63).

CREATE TABLE users (
    id             BIGINT PRIMARY KEY,
    credits        BIGINT      NOT NULL DEFAULT 0 CHECK (credits >= 0),
    xp             BIGINT      NOT NULL DEFAULT 0,
    reputation     INTEGER     NOT NULL DEFAULT 0,
    daily_streak   INTEGER     NOT NULL DEFAULT 0,
    last_daily_at  TIMESTAMPTZ,
    last_rep_at    TIMESTAMPTZ,
    bio            TEXT        NOT NULL DEFAULT '',
    accent_color   INTEGER,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX users_xp_idx ON users (xp DESC);
CREATE INDEX users_credits_idx ON users (credits DESC);

CREATE TABLE guilds (
    id                  BIGINT PRIMARY KEY,
    leveling_enabled    BOOLEAN     NOT NULL DEFAULT TRUE,
    levelup_channel_id  BIGINT,
    levelup_message     TEXT,
    welcome_enabled     BOOLEAN     NOT NULL DEFAULT FALSE,
    welcome_channel_id  BIGINT,
    welcome_message     TEXT,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Per-guild leveling. `messages` counts XP-granting messages only.
CREATE TABLE members (
    guild_id    BIGINT      NOT NULL,
    user_id     BIGINT      NOT NULL,
    xp          BIGINT      NOT NULL DEFAULT 0,
    messages    BIGINT      NOT NULL DEFAULT 0,
    last_xp_at  TIMESTAMPTZ,
    PRIMARY KEY (guild_id, user_id)
);

CREATE INDEX members_guild_xp_idx ON members (guild_id, xp DESC);

-- Every credit movement, for audits and global economy stats.
CREATE TABLE credit_ledger (
    id          BIGSERIAL PRIMARY KEY,
    user_id     BIGINT      NOT NULL,
    delta       BIGINT      NOT NULL,
    reason      TEXT        NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX credit_ledger_user_idx ON credit_ledger (user_id, created_at DESC);
