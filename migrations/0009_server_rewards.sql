-- Roles handed out at a level, per-server points, and the role store that spends them.

-- A role given when a member reaches `level`. Several roles may share a level.
CREATE TABLE level_roles (
    guild_id  BIGINT  NOT NULL,
    level     INTEGER NOT NULL CHECK (level >= 1),
    role_id   BIGINT  NOT NULL,
    PRIMARY KEY (guild_id, role_id)
);

CREATE INDEX level_roles_guild_level_idx ON level_roles (guild_id, level);

-- `level_roles_stack`: keep every earned reward (true) or only the highest one (false).
ALTER TABLE guilds
    ADD COLUMN level_roles_stack BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN points_enabled    BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN points_name       TEXT    NOT NULL DEFAULT 'points',
    ADD COLUMN points_emoji      TEXT    NOT NULL DEFAULT '⭐',
    ADD COLUMN points_min        INTEGER NOT NULL DEFAULT 1 CHECK (points_min >= 0),
    ADD COLUMN points_max        INTEGER NOT NULL DEFAULT 3,
    ADD CONSTRAINT guilds_points_range CHECK (points_max >= points_min);

-- Server points live on the member row, next to the XP they are earned with.
ALTER TABLE members ADD COLUMN points BIGINT NOT NULL DEFAULT 0 CHECK (points >= 0);

CREATE INDEX members_guild_points_idx ON members (guild_id, points DESC);

-- Roles a server sells for its points. `stock` NULL means unlimited.
CREATE TABLE store_items (
    id          BIGSERIAL   PRIMARY KEY,
    guild_id    BIGINT      NOT NULL,
    role_id     BIGINT      NOT NULL,
    price       BIGINT      NOT NULL CHECK (price > 0),
    stock       INTEGER     CHECK (stock >= 0),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (guild_id, role_id)
);
