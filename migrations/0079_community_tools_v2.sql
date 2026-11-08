-- Community tools v2. Personal data in these tables is registered in privacy.rs.
ALTER TABLE giveaways ADD COLUMN starts_at TIMESTAMPTZ;
ALTER TABLE giveaways ADD COLUMN start_announced BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE giveaways ADD COLUMN min_level INTEGER NOT NULL DEFAULT 0 CHECK (min_level BETWEEN 0 AND 1000);
ALTER TABLE giveaways ADD COLUMN min_account_days INTEGER NOT NULL DEFAULT 0 CHECK (min_account_days BETWEEN 0 AND 3650);
ALTER TABLE giveaways ADD COLUMN min_server_days INTEGER NOT NULL DEFAULT 0 CHECK (min_server_days BETWEEN 0 AND 3650);
ALTER TABLE giveaways ADD COLUMN bonus_role_id BIGINT;
ALTER TABLE giveaways ADD COLUMN bonus_entries SMALLINT NOT NULL DEFAULT 0 CHECK (bonus_entries BETWEEN 0 AND 20);
ALTER TABLE giveaway_entries ADD COLUMN weight SMALLINT NOT NULL DEFAULT 1 CHECK (weight BETWEEN 1 AND 21);
ALTER TABLE giveaways DROP CONSTRAINT giveaways_status_check;
ALTER TABLE giveaways ADD CONSTRAINT giveaways_status_check CHECK (status IN ('scheduled','running','ended'));
CREATE INDEX giveaways_start_idx ON giveaways(starts_at) WHERE status='scheduled';

ALTER TABLE polls ADD COLUMN multiple BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE polls ADD COLUMN anonymous BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE polls ADD COLUMN required_role_id BIGINT;
ALTER TABLE polls DROP CONSTRAINT polls_options_check;
ALTER TABLE polls ADD CONSTRAINT polls_options_check CHECK (cardinality(options) BETWEEN 2 AND 20);
ALTER TABLE poll_votes DROP CONSTRAINT poll_votes_choice_check;
ALTER TABLE poll_votes ADD CONSTRAINT poll_votes_choice_check CHECK (choice BETWEEN 0 AND 19);
CREATE TABLE poll_extra_votes (
    poll_id BIGINT NOT NULL REFERENCES polls(id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL,
    choice SMALLINT NOT NULL CHECK (choice BETWEEN 0 AND 19),
    PRIMARY KEY(poll_id,user_id,choice)
);

ALTER TABLE suggestions DROP CONSTRAINT suggestions_status_check;
ALTER TABLE suggestions ADD CONSTRAINT suggestions_status_check
    CHECK (status IN ('open','considering','approved','implemented','denied'));
ALTER TABLE suggestions ADD COLUMN staff_note TEXT CHECK (char_length(staff_note) <= 1000);
ALTER TABLE suggestions ADD COLUMN thread_id BIGINT;

CREATE TABLE personal_recurring_reminders (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL,
    body TEXT NOT NULL CHECK (char_length(body) BETWEEN 1 AND 200),
    interval_minutes INTEGER NOT NULL CHECK (interval_minutes BETWEEN 60 AND 525600),
    next_at TIMESTAMPTZ NOT NULL,
    lease_until TIMESTAMPTZ,
    attempts SMALLINT NOT NULL DEFAULT 0,
    language TEXT NOT NULL DEFAULT 'en',
    enabled BOOLEAN NOT NULL DEFAULT TRUE
);
CREATE INDEX recurring_reminders_due_idx ON personal_recurring_reminders(next_at) WHERE enabled;
CREATE TABLE personal_checklist_items (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL,
    body TEXT NOT NULL CHECK (char_length(body) BETWEEN 1 AND 200),
    done BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX personal_checklist_user_idx ON personal_checklist_items(user_id,done,id);

CREATE TABLE keyword_highlights (
    guild_id BIGINT NOT NULL,
    user_id BIGINT NOT NULL,
    keyword TEXT NOT NULL CHECK (char_length(keyword) BETWEEN 2 AND 40),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY(guild_id,user_id,keyword)
);
CREATE INDEX keyword_highlights_guild_idx ON keyword_highlights(guild_id);

CREATE TABLE community_auto_channels (
    guild_id BIGINT NOT NULL,
    channel_id BIGINT NOT NULL,
    auto_thread BOOLEAN NOT NULL DEFAULT FALSE,
    auto_crosspost BOOLEAN NOT NULL DEFAULT FALSE,
    thread_name TEXT NOT NULL DEFAULT 'Discussion',
    CHECK (auto_thread OR auto_crosspost),
    PRIMARY KEY(guild_id,channel_id)
);

CREATE TABLE member_anniversaries (
    guild_id BIGINT NOT NULL,
    user_id BIGINT NOT NULL,
    joined_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY(guild_id,user_id)
);
-- Invite attribution has the join time for tracked existing members; use their latest active join.
INSERT INTO member_anniversaries(guild_id,user_id,joined_at)
SELECT DISTINCT ON (guild_id,user_id) guild_id,user_id,joined_at
FROM invite_joins WHERE left_at IS NULL
ORDER BY guild_id,user_id,joined_at DESC;
CREATE TABLE anniversary_settings (
    guild_id BIGINT PRIMARY KEY,
    channel_id BIGINT,
    enabled BOOLEAN NOT NULL DEFAULT FALSE
);
CREATE TABLE anniversary_sent (
    guild_id BIGINT NOT NULL,
    user_id BIGINT NOT NULL,
    year INTEGER NOT NULL,
    PRIMARY KEY(guild_id,user_id,year)
);

CREATE TABLE confession_settings (
    guild_id BIGINT PRIMARY KEY,
    review_channel_id BIGINT,
    publish_channel_id BIGINT,
    enabled BOOLEAN NOT NULL DEFAULT FALSE
);
CREATE TABLE confessions (
    id BIGSERIAL PRIMARY KEY,
    guild_id BIGINT NOT NULL,
    author_id BIGINT NOT NULL,
    body TEXT NOT NULL CHECK (char_length(body) BETWEEN 3 AND 1000),
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','approved','denied')),
    reviewer_id BIGINT,
    reviewed_at TIMESTAMPTZ,
    review_message_id BIGINT,
    publish_message_id BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX confessions_queue_idx ON confessions(guild_id,created_at) WHERE status='pending';
CREATE TABLE confession_reveals (
    id BIGSERIAL PRIMARY KEY,
    guild_id BIGINT NOT NULL,
    confession_id BIGINT NOT NULL REFERENCES confessions(id) ON DELETE CASCADE,
    owner_id BIGINT NOT NULL,
    revealed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Reports contain no confession text or author; identity stays behind audited reveal.
CREATE TABLE confession_bans (
    guild_id BIGINT NOT NULL,
    user_id BIGINT NOT NULL,
    PRIMARY KEY(guild_id,user_id)
);
CREATE TABLE confession_reports (
    confession_id BIGINT NOT NULL REFERENCES confessions(id) ON DELETE CASCADE,
    reporter_id BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY(confession_id,reporter_id)
);
CREATE TABLE confession_moderation (
    id BIGSERIAL PRIMARY KEY,
    guild_id BIGINT NOT NULL,
    confession_id BIGINT NOT NULL REFERENCES confessions(id) ON DELETE CASCADE,
    moderator_id BIGINT NOT NULL,
    banned BOOLEAN NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE community_autopost_jobs (
    message_id BIGINT PRIMARY KEY,
    guild_id BIGINT NOT NULL,
    channel_id BIGINT NOT NULL,
    thread_pending BOOLEAN NOT NULL,
    crosspost_pending BOOLEAN NOT NULL,
    thread_name TEXT NOT NULL,
    attempts SMALLINT NOT NULL DEFAULT 0,
    next_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    lease_until TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX community_autopost_due_idx ON community_autopost_jobs(next_at)
    WHERE attempts<5 AND (thread_pending OR crosspost_pending);
