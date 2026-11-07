-- Opt-in birthdays. A per-guild/year claim makes announcement attempts unique across pods.
CREATE TABLE birthdays (
    user_id BIGINT PRIMARY KEY,
    month SMALLINT NOT NULL CHECK (month BETWEEN 1 AND 12),
    day SMALLINT NOT NULL CHECK (day BETWEEN 1 AND 31),
    year SMALLINT CHECK (year BETWEEN 1900 AND 9999),
    tz TEXT NOT NULL DEFAULT 'UTC',
    public BOOLEAN NOT NULL DEFAULT FALSE,
    show_age BOOLEAN NOT NULL DEFAULT FALSE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE birthday_settings (
    guild_id BIGINT PRIMARY KEY REFERENCES guilds(id) ON DELETE CASCADE,
    enabled BOOLEAN NOT NULL DEFAULT FALSE,
    channel_id BIGINT,
    role_id BIGINT,
    message TEXT NOT NULL DEFAULT 'Happy birthday, {user}! 🎂',
    announce_hour SMALLINT NOT NULL DEFAULT 0 CHECK (announce_hour BETWEEN 0 AND 23),
    server_tz TEXT NOT NULL DEFAULT 'UTC',
    shells_gift SMALLINT NOT NULL DEFAULT 0 CHECK (shells_gift BETWEEN 0 AND 500),
    allow_age BOOLEAN NOT NULL DEFAULT FALSE,
    badge BOOLEAN NOT NULL DEFAULT FALSE
);
CREATE TABLE birthday_sent (
    guild_id BIGINT NOT NULL REFERENCES guilds(id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL,
    year SMALLINT NOT NULL,
    role_until TIMESTAMPTZ,
    sent_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (guild_id, user_id, year)
);
CREATE TABLE birthday_gift_years (
    user_id BIGINT NOT NULL,
    year SMALLINT NOT NULL,
    PRIMARY KEY (user_id, year)
);
CREATE INDEX birthday_sent_user ON birthday_sent(user_id);
