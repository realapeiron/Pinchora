SET LOCAL lock_timeout = '5s';
CREATE TABLE workflows (
 id BIGSERIAL PRIMARY KEY, guild_id BIGINT NOT NULL REFERENCES guilds(id) ON DELETE CASCADE,
 name TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 40), enabled BOOLEAN NOT NULL DEFAULT false,
 trigger JSONB NOT NULL, conditions JSONB NOT NULL CHECK (jsonb_array_length(conditions) <= 5),
 actions JSONB NOT NULL CHECK (jsonb_array_length(actions) BETWEEN 1 AND 10),
 runs BIGINT NOT NULL DEFAULT 0, last_run_at TIMESTAMPTZ, last_error TEXT,
 created_by BIGINT NOT NULL, updated_at TIMESTAMPTZ NOT NULL DEFAULT now(), UNIQUE(guild_id,name)
);
CREATE TABLE workflow_vars (
 guild_id BIGINT NOT NULL REFERENCES guilds(id) ON DELETE CASCADE,
 scope TEXT NOT NULL CHECK (scope IN ('server','member')), subject_id BIGINT NOT NULL,
 key TEXT NOT NULL CHECK (key ~ '^[A-Za-z0-9_]{1,32}$'), value TEXT NOT NULL CHECK (char_length(value) <= 200),
 CHECK ((scope='server' AND subject_id=0) OR (scope='member' AND subject_id>0)),
 PRIMARY KEY(guild_id,scope,subject_id,key)
);
CREATE TABLE workflow_runs (
 id BIGSERIAL PRIMARY KEY, guild_id BIGINT NOT NULL REFERENCES guilds(id) ON DELETE CASCADE,
 workflow_id BIGINT NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
 trigger_summary TEXT NOT NULL, user_id BIGINT NOT NULL,
 started_at TIMESTAMPTZ NOT NULL DEFAULT now(), outcome TEXT NOT NULL, steps JSONB NOT NULL DEFAULT '[]'
);
CREATE INDEX workflow_runs_guild_time ON workflow_runs(guild_id,started_at DESC);
CREATE INDEX workflow_runs_history ON workflow_runs(workflow_id,started_at DESC);
CREATE INDEX workflow_runs_expiry ON workflow_runs(started_at);
CREATE TABLE workflow_schedules (
 workflow_id BIGINT PRIMARY KEY REFERENCES workflows(id) ON DELETE CASCADE,
 spec JSONB NOT NULL, tz TEXT NOT NULL, next_run_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX workflow_schedules_due ON workflow_schedules(next_run_at);
-- Reservations survive workflow/history deletion and hold no member identifiers.
CREATE TABLE workflow_run_budget (
 guild_id BIGINT NOT NULL REFERENCES guilds(id) ON DELETE CASCADE,
 reserved_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX workflow_run_budget_guild_time ON workflow_run_budget(guild_id,reserved_at);
-- Pending role events survive gateway delays and process restarts.
CREATE TABLE workflow_role_changes (
 guild_id BIGINT NOT NULL REFERENCES guilds(id) ON DELETE CASCADE,
 user_id BIGINT NOT NULL, role_id BIGINT NOT NULL, added BOOLEAN NOT NULL,
 pending BIGINT NOT NULL DEFAULT 1 CHECK(pending >= 0),
 PRIMARY KEY(guild_id,user_id,role_id,added)
);
