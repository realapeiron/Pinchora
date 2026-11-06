-- Moderation: numbered cases, warn thresholds, the mod-log, the detailed server log and the staff
-- action log.

-- A server's moderation settings. A missing row means: no mod-log channel of its own (cases go to the
-- server log's moderation channel, if any) and members are told by direct message.
CREATE TABLE mod_settings (
    guild_id        BIGINT  PRIMARY KEY,
    log_channel_id  BIGINT,
    dm_members      BOOLEAN NOT NULL DEFAULT TRUE
);

-- Each server counts its cases from 1. The counter row is taken (and held to the end of the
-- transaction) by the insert of every case, so two moderators acting at once get two numbers and
-- whatever counts warns after it sees the other's.
CREATE TABLE mod_case_counters (
    guild_id   BIGINT  PRIMARY KEY,
    last_case  INTEGER NOT NULL
);

-- One action taken through Pinchora. `target_id` is a member, or a channel for the channel actions
-- (purge, slowmode, lock, unlock). `active`: a warn that still counts, a ban that still stands (a
-- temporary one is lifted by the ticker at `expires_at`, which leases it for two minutes in
-- `expire_lease` so two pods never lift it twice; a ban lifted on Discord by anyone ends it). A lock
-- keeps what @everyone's overwrite allowed and denied of the locked permissions before it
-- (`lock_allow`, `lock_deny`), for the unlock to put back. The records stay when a member deletes their data:
-- they are the server's, and the moderator's name is the only thing taken out (see privacy.rs).
CREATE TABLE mod_cases (
    guild_id        BIGINT      NOT NULL,
    number          INTEGER     NOT NULL,
    kind            TEXT        NOT NULL CHECK (kind IN (
                        'ban', 'unban', 'kick', 'timeout', 'untimeout', 'warn', 'unwarn', 'purge',
                        'slowmode', 'lock', 'unlock', 'nick', 'role_add', 'role_remove')),
    target_id       BIGINT      NOT NULL,
    target_name     TEXT        NOT NULL,
    moderator_id    BIGINT      NOT NULL,
    moderator_name  TEXT        NOT NULL,
    reason          TEXT        CHECK (char_length(reason) <= 512),
    duration_secs   BIGINT      CHECK (duration_secs > 0),
    expires_at      TIMESTAMPTZ,
    details         TEXT,
    active          BOOLEAN     NOT NULL DEFAULT TRUE,
    dm_sent         BOOLEAN,
    log_channel_id  BIGINT,
    log_message_id  BIGINT,
    expire_lease    TIMESTAMPTZ,
    lock_allow      BIGINT,
    lock_deny       BIGINT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ,
    PRIMARY KEY (guild_id, number)
);

CREATE INDEX mod_cases_target_idx ON mod_cases (guild_id, target_id, number DESC);
CREATE INDEX mod_cases_kind_idx ON mod_cases (guild_id, kind, number DESC);
CREATE INDEX mod_cases_expiring_idx ON mod_cases (expires_at) WHERE kind = 'ban' AND active AND expires_at IS NOT NULL;

-- What a number of warns brings: a timeout (for `duration_secs`), a kick, or a ban (temporary with a
-- duration). Fired when a warn makes a member's active warns reach `warns` exactly.
CREATE TABLE mod_warn_thresholds (
    guild_id       BIGINT  NOT NULL,
    warns          INTEGER NOT NULL CHECK (warns BETWEEN 1 AND 50),
    action         TEXT    NOT NULL CHECK (action IN ('timeout', 'kick', 'ban')),
    duration_secs  BIGINT  CHECK (duration_secs > 0),
    PRIMARY KEY (guild_id, warns)
);

-- The detailed server log. The channel of each category; a category without one uses the server
-- log's channel (`guilds.log_channel_id`).
CREATE TABLE log_channels (
    guild_id    BIGINT NOT NULL,
    category    TEXT   NOT NULL,
    channel_id  BIGINT NOT NULL,
    PRIMARY KEY (guild_id, category)
);

-- Events switched off one by one (the four older switches in `guilds` still cover their groups),
-- and who and where the log leaves alone. Self mute and deafen are off until a server wants them.
CREATE TABLE log_settings (
    guild_id          BIGINT   PRIMARY KEY,
    disabled_events   TEXT[]   NOT NULL DEFAULT '{voice_self}',
    ignored_channels  BIGINT[] NOT NULL DEFAULT '{}' CHECK (cardinality(ignored_channels) <= 50),
    ignored_roles     BIGINT[] NOT NULL DEFAULT '{}' CHECK (cardinality(ignored_roles) <= 50),
    ignored_users     BIGINT[] NOT NULL DEFAULT '{}' CHECK (cardinality(ignored_users) <= 50),
    ignore_bots       BOOLEAN  NOT NULL DEFAULT FALSE
);

-- A server that already logged keeps logging what it did: the four older groups (joins and leaves,
-- names, bans, voice joins, leaves and moves). Every newer event starts off there until it is switched on;
-- a server that picks its first log channel from now on gets them all.
INSERT INTO log_settings (guild_id, disabled_events)
SELECT id, ARRAY[
    'avatar', 'message_edit', 'message_delete', 'message_bulk_delete', 'role_create', 'role_delete',
    'role_update', 'member_roles', 'channel_create', 'channel_delete', 'channel_update', 'channel_permissions',
    'thread_create', 'thread_delete', 'thread_update', 'voice_moderation', 'voice_self', 'kick', 'timeout',
    'server_update', 'invite_create', 'invite_delete', 'emojis', 'stickers'
]
FROM guilds WHERE log_channel_id IS NOT NULL;

-- What staff did: moderation commands (`command`, with their case), what Discord's audit log says a
-- member did by hand (`discord`) and cases edited on the dashboard (`web`). The dashboard's settings
-- changes stay in `dashboard_audit` and are read together with these. Kept 180 days.
CREATE TABLE staff_actions (
    id           BIGSERIAL   PRIMARY KEY,
    guild_id     BIGINT      NOT NULL,
    actor_id     BIGINT      NOT NULL,
    actor_name   TEXT        NOT NULL,
    source       TEXT        NOT NULL CHECK (source IN ('command', 'discord', 'web')),
    action       TEXT        NOT NULL,
    target_id    BIGINT,
    target_name  TEXT,
    case_number  INTEGER,
    reason       TEXT,
    details      TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX staff_actions_guild_idx ON staff_actions (guild_id, id DESC);
CREATE INDEX staff_actions_created_idx ON staff_actions (created_at);
