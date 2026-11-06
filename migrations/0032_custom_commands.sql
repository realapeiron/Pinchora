-- Custom commands (`/tag`) and auto-responders: text, embeds and a few actions a server writes itself.
-- Rules the bot cannot express in SQL (placeholders, action shapes, embed limits) live in
-- `pinchora_core::tags`; the checks here are the last line against a bug.
CREATE TABLE custom_commands (
    id             BIGSERIAL   PRIMARY KEY,
    guild_id       BIGINT      NOT NULL,
    name           TEXT        NOT NULL CHECK (name ~ '^[a-z0-9][a-z0-9_-]{0,31}$'),
    description    TEXT        NOT NULL DEFAULT '' CHECK (char_length(description) <= 100),
    -- One is picked at random; a reply may be empty only when there is an embed.
    responses      TEXT[]      NOT NULL CHECK (cardinality(responses) BETWEEN 1 AND 5),
    embed          JSONB,
    reply_mode     TEXT        NOT NULL DEFAULT 'public' CHECK (reply_mode IN ('public', 'ephemeral')),
    -- At most 3 actions: add_role, remove_role, toggle_role, give_points, dm.
    actions        JSONB       NOT NULL DEFAULT '[]' CHECK (jsonb_typeof(actions) = 'array' AND jsonb_array_length(actions) <= 3),
    allow_roles    BIGINT[]    NOT NULL DEFAULT '{}' CHECK (cardinality(allow_roles) <= 25),
    deny_roles     BIGINT[]    NOT NULL DEFAULT '{}' CHECK (cardinality(deny_roles) <= 25),
    allow_channels BIGINT[]    NOT NULL DEFAULT '{}' CHECK (cardinality(allow_channels) <= 25),
    deny_channels  BIGINT[]    NOT NULL DEFAULT '{}' CHECK (cardinality(deny_channels) <= 25),
    cooldown_secs  INTEGER     NOT NULL DEFAULT 0 CHECK (cooldown_secs BETWEEN 0 AND 3600),
    enabled        BOOLEAN     NOT NULL DEFAULT TRUE,
    -- Whether the text may ping the users and roles it names (never everyone or here).
    allow_mentions BOOLEAN     NOT NULL DEFAULT FALSE,
    uses           BIGINT      NOT NULL DEFAULT 0,
    created_by     BIGINT      NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (guild_id, name)
);

CREATE TABLE custom_responders (
    id             BIGSERIAL   PRIMARY KEY,
    guild_id       BIGINT      NOT NULL,
    name           TEXT        NOT NULL CHECK (char_length(name) BETWEEN 1 AND 40),
    trigger_kind   TEXT        NOT NULL CHECK (trigger_kind IN ('member_join', 'member_leave', 'level_up', 'role_added', 'role_removed', 'keyword')),
    -- A role id for role_*, the keyword for keyword, the lowest level for level_up.
    trigger_value  TEXT,
    match_kind     TEXT        CHECK (match_kind IN ('contains', 'exact', 'starts_with', 'ends_with')),
    responses      TEXT[]      NOT NULL CHECK (cardinality(responses) BETWEEN 1 AND 5),
    embed          JSONB,
    destination    TEXT        NOT NULL DEFAULT 'current' CHECK (destination IN ('current', 'channel', 'dm')),
    channel_id     BIGINT,
    actions        JSONB       NOT NULL DEFAULT '[]' CHECK (jsonb_typeof(actions) = 'array' AND jsonb_array_length(actions) <= 3),
    chance         SMALLINT    NOT NULL DEFAULT 100 CHECK (chance BETWEEN 1 AND 100),
    cooldown_secs  INTEGER     NOT NULL DEFAULT 0 CHECK (cooldown_secs BETWEEN 0 AND 86400),
    allow_channels BIGINT[]    NOT NULL DEFAULT '{}' CHECK (cardinality(allow_channels) <= 25),
    deny_channels  BIGINT[]    NOT NULL DEFAULT '{}' CHECK (cardinality(deny_channels) <= 25),
    enabled        BOOLEAN     NOT NULL DEFAULT TRUE,
    allow_mentions BOOLEAN     NOT NULL DEFAULT FALSE,
    created_by     BIGINT      NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX custom_responders_guild_idx ON custom_responders (guild_id, trigger_kind) WHERE enabled;
