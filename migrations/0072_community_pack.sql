CREATE TABLE qotd_settings (
    guild_id BIGINT PRIMARY KEY, channel_id BIGINT, ping_role_id BIGINT,
    hour SMALLINT NOT NULL DEFAULT 12 CHECK (hour BETWEEN 0 AND 23),
    timezone TEXT NOT NULL DEFAULT 'UTC', thread_per_question BOOLEAN NOT NULL DEFAULT FALSE,
    last_posted_on DATE
);
CREATE TABLE qotd_questions (
    id BIGSERIAL PRIMARY KEY, guild_id BIGINT NOT NULL, text TEXT NOT NULL CHECK (char_length(text) BETWEEN 1 AND 300),
    text_tr TEXT CHECK (text_tr IS NULL OR char_length(text_tr) BETWEEN 1 AND 300),
    suggested_by BIGINT, approved BOOLEAN NOT NULL DEFAULT FALSE, asked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX qotd_queue ON qotd_questions(guild_id, approved, asked_at, id);
CREATE TABLE counting_channels (
    channel_id BIGINT PRIMARY KEY, guild_id BIGINT NOT NULL UNIQUE, current BIGINT NOT NULL DEFAULT 0,
    last_user_id BIGINT, record BIGINT NOT NULL DEFAULT 0, record_at TIMESTAMPTZ,
    reset_on_fail BOOLEAN NOT NULL DEFAULT TRUE, allow_math BOOLEAN NOT NULL DEFAULT FALSE, fail_role_id BIGINT
);
CREATE TABLE counting_stats (
    guild_id BIGINT NOT NULL, user_id BIGINT NOT NULL, correct BIGINT NOT NULL DEFAULT 0,
    fails BIGINT NOT NULL DEFAULT 0, PRIMARY KEY(guild_id,user_id)
);
CREATE TABLE forms (
    id BIGSERIAL PRIMARY KEY, guild_id BIGINT NOT NULL, name TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    questions JSONB NOT NULL DEFAULT '[]', submit_channel_id BIGINT, review_roles BIGINT[] NOT NULL DEFAULT '{}',
    accept_role_id BIGINT, dm_result BOOLEAN NOT NULL DEFAULT TRUE, open BOOLEAN NOT NULL DEFAULT TRUE,
    message_id BIGINT, UNIQUE(guild_id,name)
);
CREATE TABLE form_submissions (
    id BIGSERIAL PRIMARY KEY, form_id BIGINT NOT NULL REFERENCES forms(id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL, answers JSONB NOT NULL, status TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN ('pending','accepted','denied')),
    decided_by BIGINT, reason TEXT, created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX form_submissions_form_status ON form_submissions(form_id,status,created_at);
CREATE TABLE modmail_settings (
    guild_id BIGINT PRIMARY KEY, enabled BOOLEAN NOT NULL DEFAULT FALSE, forum_channel_id BIGINT,
    staff_roles BIGINT[] NOT NULL DEFAULT '{}', greeting TEXT NOT NULL DEFAULT '',
    close_after_hours INT NOT NULL DEFAULT 72 CHECK(close_after_hours BETWEEN 1 AND 8760),
    anonymous_staff BOOLEAN NOT NULL DEFAULT FALSE, log_channel_id BIGINT
);
CREATE TABLE modmail_threads (
    id BIGSERIAL PRIMARY KEY, guild_id BIGINT NOT NULL, user_id BIGINT NOT NULL,
    thread_id BIGINT NOT NULL UNIQUE, status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','closed')),
    opened_at TIMESTAMPTZ NOT NULL DEFAULT now(), closed_at TIMESTAMPTZ, closed_by BIGINT
);
CREATE UNIQUE INDEX modmail_one_open ON modmail_threads(guild_id,user_id) WHERE status='open';
CREATE TABLE modmail_snippets (
    guild_id BIGINT NOT NULL, name TEXT NOT NULL, text TEXT NOT NULL,
    PRIMARY KEY(guild_id,name)
);
CREATE TABLE modmail_blocks (
    guild_id BIGINT NOT NULL, user_id BIGINT NOT NULL, PRIMARY KEY(guild_id,user_id)
);
CREATE TABLE modmail_pending (
    user_id BIGINT PRIMARY KEY, body TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
