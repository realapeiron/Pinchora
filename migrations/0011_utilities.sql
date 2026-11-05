-- Server utilities: a role for new members, goodbye messages, the server log and role button panels.

-- Join role and goodbye are on while their role / channel is set. The log has one switch per kind of
-- event; all are on by default, so picking a log channel is enough to start logging.
ALTER TABLE guilds
    ADD COLUMN join_role_id       BIGINT,
    ADD COLUMN goodbye_channel_id BIGINT,
    ADD COLUMN goodbye_message    TEXT,
    ADD COLUMN log_channel_id     BIGINT,
    ADD COLUMN log_members        BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN log_names          BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN log_bans           BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN log_voice          BOOLEAN NOT NULL DEFAULT TRUE;

-- A message of role buttons. It stays a draft until it is posted; `channel_id` and `message_id`
-- then point at the posted message so posting again updates it.
CREATE TABLE role_panels (
    id           BIGSERIAL   PRIMARY KEY,
    guild_id     BIGINT      NOT NULL,
    title        TEXT        NOT NULL,
    description  TEXT,
    channel_id   BIGINT,
    message_id   BIGINT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Panels are found by title, ignoring case.
CREATE UNIQUE INDEX role_panels_title_idx ON role_panels (guild_id, lower(title));

-- One button per role. `position` keeps the buttons in the order they were added.
CREATE TABLE role_panel_roles (
    panel_id  BIGINT  NOT NULL REFERENCES role_panels (id) ON DELETE CASCADE,
    role_id   BIGINT  NOT NULL,
    label     TEXT    NOT NULL,
    emoji     TEXT,
    position  INTEGER NOT NULL,
    PRIMARY KEY (panel_id, role_id)
);
