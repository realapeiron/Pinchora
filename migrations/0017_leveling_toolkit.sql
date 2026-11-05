-- Leveling toolkit: XP multipliers and no-XP channels and roles, voice XP, level-up message modes,
-- weekly XP boards and reputation reward roles.

-- `levelup_mode`: where a level-up is announced. `channel` is the level-up channel (or the channel
-- the member chatted in when none is set), `current` always where they chatted, `dm` a direct
-- message, `off` nowhere. Voice XP is off until a server turns it on; `voice_xp_amount` is per minute.
ALTER TABLE guilds
    ADD COLUMN levelup_mode     TEXT    NOT NULL DEFAULT 'channel'
        CHECK (levelup_mode IN ('channel', 'current', 'dm', 'off')),
    ADD COLUMN voice_xp_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN voice_xp_amount  INTEGER NOT NULL DEFAULT 10
        CHECK (voice_xp_amount BETWEEN 1 AND 100);

-- A multiplier on the server XP and points a channel or role earns. 0 means no XP at all; a rule
-- of 1 is the default and is never stored.
CREATE TABLE xp_rules (
    guild_id    BIGINT           NOT NULL,
    kind        TEXT             NOT NULL CHECK (kind IN ('channel', 'role')),
    target_id   BIGINT           NOT NULL,
    multiplier  DOUBLE PRECISION NOT NULL CHECK (multiplier >= 0 AND multiplier <= 5),
    PRIMARY KEY (guild_id, kind, target_id)
);

-- Server XP earned per week. `week` is the Monday (UTC) the week starts on, so a new week simply
-- starts counting under a new key; old weeks stay for the "last week" recap and are pruned later.
CREATE TABLE weekly_xp (
    guild_id  BIGINT NOT NULL,
    week      DATE   NOT NULL,
    user_id   BIGINT NOT NULL,
    xp        BIGINT NOT NULL DEFAULT 0 CHECK (xp >= 0),
    PRIMARY KEY (guild_id, week, user_id)
);

CREATE INDEX weekly_xp_board_idx ON weekly_xp (guild_id, week, xp DESC);

-- A role given when a member's reputation reaches `reputation`. Several roles may share a count.
CREATE TABLE rep_roles (
    guild_id    BIGINT  NOT NULL,
    reputation  INTEGER NOT NULL CHECK (reputation >= 1),
    role_id     BIGINT  NOT NULL,
    PRIMARY KEY (guild_id, role_id)
);

CREATE INDEX rep_roles_guild_idx ON rep_roles (guild_id, reputation);
