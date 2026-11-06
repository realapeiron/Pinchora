-- Tickets: panels members open a private ticket from (a channel or a private thread each), the form a
-- panel asks before opening, the tickets themselves and the transcript kept when one closes.

-- One row per server that changed anything; a server without one runs on the defaults.
CREATE TABLE ticket_settings (
    guild_id BIGINT PRIMARY KEY,
    -- Where ticket channels are made, and where closed ones move to (NULL: they stay put).
    open_category_id BIGINT,
    closed_category_id BIGINT,
    -- Opened, claimed and closed tickets are logged here, closed ones with their transcript.
    log_channel_id BIGINT,
    -- An open ticket nobody wrote in for this long closes by itself; 0 never.
    auto_close_hours INTEGER NOT NULL DEFAULT 0 CHECK (auto_close_hours BETWEEN 0 AND 720),
    dm_transcript BOOLEAN NOT NULL DEFAULT TRUE,
    rating_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    -- The new channel's name, with {number}, {user} and {topic}.
    naming TEXT NOT NULL DEFAULT 'ticket-{number}',
    -- `auto` makes private threads once the server has more than 400 channels.
    mode TEXT NOT NULL DEFAULT 'auto' CHECK (mode IN ('auto', 'channel', 'thread')),
    max_open_per_user INTEGER NOT NULL DEFAULT 1 CHECK (max_open_per_user BETWEEN 1 AND 10),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- The questions a panel asks in a form before the ticket opens (at most five, Discord's limit).
CREATE TABLE ticket_forms (
    id BIGSERIAL PRIMARY KEY,
    guild_id BIGINT NOT NULL,
    name TEXT NOT NULL,
    questions JSONB NOT NULL DEFAULT '[]',
    UNIQUE (guild_id, name)
);

-- At most ten per server. `buttons` are the topics: up to five buttons, or up to 25 options of a menu.
CREATE TABLE ticket_panels (
    id BIGSERIAL PRIMARY KEY,
    guild_id BIGINT NOT NULL,
    name TEXT NOT NULL,
    channel_id BIGINT NOT NULL,
    message_id BIGINT,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    color INTEGER NOT NULL DEFAULT 16739146,
    style TEXT NOT NULL DEFAULT 'buttons' CHECK (style IN ('buttons', 'select')),
    buttons JSONB NOT NULL DEFAULT '[]',
    form_id BIGINT REFERENCES ticket_forms (id) ON DELETE SET NULL,
    support_roles BIGINT[] NOT NULL DEFAULT '{}',
    welcome_text TEXT NOT NULL DEFAULT '',
    -- Overrides of the server's category and log channel for this panel's tickets.
    open_category_id BIGINT,
    log_channel_id BIGINT,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (guild_id, name)
);

-- The number the next ticket of a server gets.
CREATE TABLE ticket_counters (
    guild_id BIGINT PRIMARY KEY,
    next INTEGER NOT NULL
);

CREATE TABLE tickets (
    id BIGSERIAL PRIMARY KEY,
    guild_id BIGINT NOT NULL,
    panel_id BIGINT REFERENCES ticket_panels (id) ON DELETE SET NULL,
    number INTEGER NOT NULL,
    opener_id BIGINT NOT NULL,
    -- The ticket's own channel, or (with `thread_id`) the channel its private thread hangs off.
    channel_id BIGINT,
    thread_id BIGINT,
    topic TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'closed')),
    locked BOOLEAN NOT NULL DEFAULT FALSE,
    claimed_by BIGINT,
    closed_by BIGINT,
    close_reason TEXT,
    opened_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    closed_at TIMESTAMPTZ,
    last_activity_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- The first message somebody other than the opener (and not a bot) wrote, read from the history.
    first_reply_at TIMESTAMPTZ,
    form_answers JSONB NOT NULL DEFAULT '[]',
    rating SMALLINT CHECK (rating BETWEEN 1 AND 5),
    UNIQUE (guild_id, number)
);
CREATE INDEX tickets_open_by ON tickets (guild_id, opener_id) WHERE status = 'open';
CREATE INDEX tickets_place ON tickets ((COALESCE(thread_id, channel_id)));
CREATE INDEX tickets_idle ON tickets (last_activity_at) WHERE status = 'open';
CREATE INDEX tickets_guild ON tickets (guild_id, id DESC);

-- The messages of a closed ticket. Authors are numbers into `ticket_transcript_authors`, so a member
-- who deletes their data leaves the transcript as "Deleted user".
CREATE TABLE ticket_transcripts (
    ticket_id BIGINT PRIMARY KEY REFERENCES tickets (id) ON DELETE CASCADE,
    json JSONB NOT NULL,
    messages INTEGER NOT NULL,
    -- Without the Message Content intent only authors, times and attachment names are kept.
    with_content BOOLEAN NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE ticket_transcript_authors (
    ticket_id BIGINT NOT NULL REFERENCES tickets (id) ON DELETE CASCADE,
    idx SMALLINT NOT NULL,
    user_id BIGINT NOT NULL,
    name TEXT NOT NULL,
    bot BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (ticket_id, idx)
);
CREATE INDEX ticket_transcript_authors_user ON ticket_transcript_authors (user_id);
