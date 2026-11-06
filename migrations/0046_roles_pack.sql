-- Roles pack: auto roles, role panels with a style and rules (buttons, select menu, reactions), roles
-- that expire, roles members keep when they leave and come back, and colour roles.

-- Whether a server gives members back the roles they held when they left (`/autoroles sticky`).
ALTER TABLE guilds ADD COLUMN sticky_roles BOOLEAN NOT NULL DEFAULT FALSE;

-- Roles given on their own: to a human who joins (`join`), to a bot that joins (`bot`) or to a member
-- who passes the join gate (`after_verify`). `delay_secs` 0 gives it at once; `remove_after_secs` 0
-- keeps it for good.
CREATE TABLE auto_roles (
    id                BIGSERIAL PRIMARY KEY,
    guild_id          BIGINT  NOT NULL,
    role_id           BIGINT  NOT NULL,
    kind              TEXT    NOT NULL CHECK (kind IN ('join', 'bot', 'after_verify')),
    delay_secs        INTEGER NOT NULL DEFAULT 0 CHECK (delay_secs BETWEEN 0 AND 2592000),
    remove_after_secs INTEGER NOT NULL DEFAULT 0 CHECK (remove_after_secs BETWEEN 0 AND 31536000),
    enabled           BOOLEAN NOT NULL DEFAULT TRUE,
    UNIQUE (guild_id, role_id, kind)
);

-- What a panel is: its style, and the rules a member's picks follow. Panels made before this
-- migration are plain buttons with no rules, exactly as they were.
ALTER TABLE role_panels
    ADD COLUMN style         TEXT    NOT NULL DEFAULT 'buttons' CHECK (style IN ('buttons', 'select', 'reactions')),
    ADD COLUMN exclusive     BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN required_role BIGINT,
    ADD COLUMN min_picks     INTEGER NOT NULL DEFAULT 0 CHECK (min_picks BETWEEN 0 AND 25),
    ADD COLUMN max_picks     INTEGER NOT NULL DEFAULT 0 CHECK (max_picks BETWEEN 0 AND 25);

-- A reaction on a panel's message is found by the message.
CREATE INDEX role_panels_message_idx ON role_panels (message_id) WHERE message_id IS NOT NULL;

-- One entry of a panel: a line for a select menu, an emoji id for a custom emoji (the emoji itself is
-- still the `emoji` text), and how long the role lasts once a member took it from this panel.
ALTER TABLE role_panel_roles
    ADD COLUMN description TEXT,
    ADD COLUMN emoji_id    BIGINT,
    ADD COLUMN temp_secs   INTEGER CHECK (temp_secs IS NULL OR temp_secs BETWEEN 60 AND 31536000);

-- The reactions the bot put on a posted reactions panel, and the role each one gives. `emoji` is
-- `u:<unicode>` or `c:<custom emoji id>`.
CREATE TABLE panel_reactions (
    panel_id BIGINT NOT NULL REFERENCES role_panels (id) ON DELETE CASCADE,
    emoji    TEXT   NOT NULL,
    role_id  BIGINT NOT NULL,
    PRIMARY KEY (panel_id, emoji)
);

-- The roles a member held, kept while sticky roles are on. `left_at` is set when they leave; a row
-- that is not left yet is the last thing the bot saw them hold.
CREATE TABLE sticky_roles (
    guild_id BIGINT      NOT NULL,
    user_id  BIGINT      NOT NULL,
    role_ids BIGINT[]    NOT NULL DEFAULT '{}',
    left_at  TIMESTAMPTZ,
    PRIMARY KEY (guild_id, user_id)
);
CREATE INDEX sticky_roles_left_idx ON sticky_roles (left_at) WHERE left_at IS NOT NULL;

-- Roles to take away at a time. Several parts of the bot use it (a panel's expiring role, an auto
-- role's `remove_after`, later a birthday role). `lease_until` keeps two pods from taking one twice.
CREATE TABLE timed_roles (
    id          BIGSERIAL   PRIMARY KEY,
    guild_id    BIGINT      NOT NULL,
    user_id     BIGINT      NOT NULL,
    role_id     BIGINT      NOT NULL,
    remove_at   TIMESTAMPTZ NOT NULL,
    lease_until TIMESTAMPTZ,
    UNIQUE (guild_id, user_id, role_id)
);
CREATE INDEX timed_roles_due_idx ON timed_roles (remove_at);

-- Auto roles that wait for their delay, and what to do with them afterwards.
CREATE TABLE delayed_roles (
    id                BIGSERIAL   PRIMARY KEY,
    guild_id          BIGINT      NOT NULL,
    user_id           BIGINT      NOT NULL,
    role_id           BIGINT      NOT NULL,
    grant_at          TIMESTAMPTZ NOT NULL,
    remove_after_secs INTEGER     NOT NULL DEFAULT 0,
    lease_until       TIMESTAMPTZ,
    UNIQUE (guild_id, user_id, role_id)
);
CREATE INDEX delayed_roles_due_idx ON delayed_roles (grant_at);

-- Colour roles. `allowed_roles` empty means every member may pick; `mode` is the palette only, or
-- any hex colour too; new roles are made under `anchor_role_id` when there is one.
CREATE TABLE color_settings (
    guild_id       BIGINT   PRIMARY KEY,
    enabled        BOOLEAN  NOT NULL DEFAULT FALSE,
    allowed_roles  BIGINT[] NOT NULL DEFAULT '{}' CHECK (cardinality(allowed_roles) <= 10),
    mode           TEXT     NOT NULL DEFAULT 'palette' CHECK (mode IN ('palette', 'hex')),
    anchor_role_id BIGINT
);

-- The roles that are colours: one role per colour, at most 32 a server (the code checks the count).
CREATE TABLE color_roles (
    guild_id BIGINT NOT NULL,
    role_id  BIGINT NOT NULL,
    hex      TEXT   NOT NULL CHECK (hex ~ '^#[0-9a-f]{6}$'),
    name     TEXT   NOT NULL,
    PRIMARY KEY (guild_id, role_id)
);
CREATE UNIQUE INDEX color_roles_hex_idx ON color_roles (guild_id, hex);
