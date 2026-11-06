-- Server stats (F9): what a server's members do, counted without reading message text, and counter
-- channels whose names show a live number. The bot counts in Redis and flushes here once a minute;
-- the hourly and daily numbers are kept 90 days.

-- One server's hour (UTC): messages, voice minutes, joins, leaves and commands.
CREATE TABLE guild_activity_hourly (
    guild_id      BIGINT      NOT NULL,
    hour          TIMESTAMPTZ NOT NULL,
    messages      INTEGER     NOT NULL DEFAULT 0,
    voice_minutes INTEGER     NOT NULL DEFAULT 0,
    joins         INTEGER     NOT NULL DEFAULT 0,
    leaves        INTEGER     NOT NULL DEFAULT 0,
    commands      INTEGER     NOT NULL DEFAULT 0,
    PRIMARY KEY (guild_id, hour)
);

CREATE INDEX guild_activity_hourly_hour_idx ON guild_activity_hourly (hour);

-- One channel's day (UTC). A thread counts in the channel it belongs to.
CREATE TABLE channel_activity_daily (
    guild_id      BIGINT  NOT NULL,
    channel_id    BIGINT  NOT NULL,
    day           DATE    NOT NULL,
    messages      INTEGER NOT NULL DEFAULT 0,
    voice_minutes INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (guild_id, channel_id, day)
);

CREATE INDEX channel_activity_daily_day_idx ON channel_activity_daily (day);

-- One member's day (UTC) in one server. "Delete my data" removes these (privacy.rs).
CREATE TABLE member_activity_daily (
    guild_id      BIGINT  NOT NULL,
    user_id       BIGINT  NOT NULL,
    day           DATE    NOT NULL,
    messages      INTEGER NOT NULL DEFAULT 0,
    voice_minutes INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (guild_id, user_id, day)
);

CREATE INDEX member_activity_daily_day_idx ON member_activity_daily (day);
CREATE INDEX member_activity_daily_user_idx ON member_activity_daily (user_id);

-- The Redis batches already written, so a flush cut short and run again counts nothing twice.
CREATE TABLE stats_flushes (
    batch      TEXT        PRIMARY KEY,
    flushed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Channels whose name shows a number ("Members: 1,234"), at most ten per server.
CREATE TABLE counter_channels (
    id             BIGSERIAL   PRIMARY KEY,
    guild_id       BIGINT      NOT NULL,
    channel_id     BIGINT      NOT NULL,
    kind           TEXT        NOT NULL,
    role_id        BIGINT,
    template       TEXT        NOT NULL,
    last_value     BIGINT,
    last_name      TEXT,
    last_rename_at TIMESTAMPTZ,
    checked_at     TIMESTAMPTZ,
    problem        TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (guild_id, channel_id)
);

-- Whether anyone with the link may see the server's stats page.
ALTER TABLE guilds ADD COLUMN stats_public BOOLEAN NOT NULL DEFAULT FALSE;

-- The whole bot's totals, one row a day, for the public stats page.
CREATE TABLE global_stats_daily (
    day      DATE   PRIMARY KEY,
    servers  BIGINT NOT NULL,
    players  BIGINT NOT NULL,
    houses   BIGINT NOT NULL,
    fish     BIGINT NOT NULL,
    games    BIGINT NOT NULL,
    messages BIGINT NOT NULL DEFAULT 0
);
