-- Who changed what on the web dashboard, and when. `before`/`after` hold only the changed fields.
CREATE TABLE dashboard_audit (
    id          BIGSERIAL   PRIMARY KEY,
    guild_id    BIGINT      NOT NULL,
    actor_id    BIGINT      NOT NULL,
    actor_name  TEXT        NOT NULL,            -- as seen when the change was made
    source      TEXT        NOT NULL CHECK (source IN ('web', 'discord')),
    action      TEXT        NOT NULL,            -- 'settings.patch', 'module.set', 'level_roles.put', ...
    target      TEXT,                            -- role id, module key, command name
    before      JSONB,
    after       JSONB,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX dashboard_audit_guild_idx ON dashboard_audit (guild_id, id DESC);
CREATE INDEX dashboard_audit_created_idx ON dashboard_audit (created_at);
