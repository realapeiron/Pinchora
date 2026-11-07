-- Temp voice: creator channels that make a voice room for whoever joins them, the rooms that exist right
-- now, and the members an owner kicked out of one.

-- A voice channel a server set up as a creator. Joining it makes a room for the member and moves them in.
-- `category_id` is where the rooms go (NULL: at the top); `panel` posts the control buttons in the room,
-- or in a private thread of `panel_channel_id` (a text channel) when there is one.
CREATE TABLE voice_creators (
    guild_id         BIGINT      NOT NULL,
    channel_id       BIGINT      PRIMARY KEY,
    category_id      BIGINT,
    name_template    TEXT        NOT NULL DEFAULT '{user}''s room',
    default_limit    INT         NOT NULL DEFAULT 0 CHECK (default_limit BETWEEN 0 AND 99),
    default_locked   BOOLEAN     NOT NULL DEFAULT FALSE,
    max_per_user     INT         NOT NULL DEFAULT 1 CHECK (max_per_user BETWEEN 1 AND 5),
    panel            BOOLEAN     NOT NULL DEFAULT TRUE,
    panel_channel_id BIGINT,
    enabled          BOOLEAN     NOT NULL DEFAULT TRUE,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX voice_creators_guild_idx ON voice_creators (guild_id);

-- A room that exists right now. `source_id` is the creator that made it (it may be removed while rooms
-- live, so no foreign key). `owner_id` is NULL when nobody owns the room (the owner deleted their data):
-- whoever is in it may claim it. The panel and the thread sit in `panel_channel_id` / `text_channel_id`.
CREATE TABLE temp_voice_channels (
    channel_id       BIGINT      PRIMARY KEY,
    guild_id         BIGINT      NOT NULL,
    source_id        BIGINT      NOT NULL,
    owner_id         BIGINT,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    locked           BOOLEAN     NOT NULL DEFAULT FALSE,
    hidden           BOOLEAN     NOT NULL DEFAULT FALSE,
    user_limit       INT         NOT NULL DEFAULT 0 CHECK (user_limit BETWEEN 0 AND 99),
    last_rename_at   TIMESTAMPTZ,
    text_channel_id  BIGINT,
    panel_channel_id BIGINT,
    panel_message_id BIGINT
);
CREATE INDEX temp_voice_channels_guild_idx ON temp_voice_channels (guild_id, created_at);
CREATE INDEX temp_voice_channels_owner_idx ON temp_voice_channels (guild_id, owner_id);

-- Members an owner kicked from a room: they cannot come back while it lasts.
CREATE TABLE temp_voice_bans (
    channel_id BIGINT NOT NULL REFERENCES temp_voice_channels (channel_id) ON DELETE CASCADE,
    user_id    BIGINT NOT NULL,
    PRIMARY KEY (channel_id, user_id)
);
CREATE INDEX temp_voice_bans_user_idx ON temp_voice_bans (user_id);
