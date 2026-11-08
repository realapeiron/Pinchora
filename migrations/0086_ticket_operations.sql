-- Ticket operation state is separate from the original ticket row so existing panels and transcripts remain compatible.
CREATE TABLE ticket_operation_settings (
    guild_id BIGINT PRIMARY KEY,
    support_timezone TEXT NOT NULL DEFAULT 'UTC',
    support_hours JSONB NOT NULL DEFAULT '[]',
    out_of_hours_message TEXT NOT NULL DEFAULT '',
    status_channel_id BIGINT,
    status_message_id BIGINT,
    close_timeout_hours INTEGER NOT NULL DEFAULT 24 CHECK (close_timeout_hours BETWEEN 1 AND 168),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE ticket_panel_operations (
    panel_id BIGINT PRIMARY KEY REFERENCES ticket_panels(id) ON DELETE CASCADE,
    claim_mode TEXT NOT NULL DEFAULT 'optional' CHECK (claim_mode IN ('optional', 'required', 'exclusive')),
    max_open_per_panel INTEGER NOT NULL DEFAULT 0 CHECK (max_open_per_panel BETWEEN 0 AND 100),
    max_open_per_category INTEGER NOT NULL DEFAULT 0 CHECK (max_open_per_category BETWEEN 0 AND 100),
    cooldown_seconds INTEGER NOT NULL DEFAULT 60 CHECK (cooldown_seconds BETWEEN 0 AND 86400),
    minimum_account_age_days INTEGER NOT NULL DEFAULT 0 CHECK (minimum_account_age_days BETWEEN 0 AND 3650),
    minimum_membership_days INTEGER NOT NULL DEFAULT 0 CHECK (minimum_membership_days BETWEEN 0 AND 3650),
    bypass_roles BIGINT[] NOT NULL DEFAULT '{}',
    staff_open_limit INTEGER NOT NULL DEFAULT 0 CHECK (staff_open_limit BETWEEN 0 AND 100),
    overflow_message TEXT NOT NULL DEFAULT '',
    faq JSONB NOT NULL DEFAULT '[]',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE ticket_operations (
    ticket_id BIGINT PRIMARY KEY REFERENCES tickets(id) ON DELETE CASCADE,
    priority TEXT NOT NULL DEFAULT 'normal' CHECK (priority IN ('low', 'normal', 'high', 'urgent')),
    escalated_at TIMESTAMPTZ,
    transferred_to_user BIGINT,
    transferred_to_role BIGINT,
    close_requested_by BIGINT,
    close_requested_at TIMESTAMPTZ,
    close_request_expires_at TIMESTAMPTZ,
    force_opened_by BIGINT,
    last_reminder_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (transferred_to_user IS NULL OR transferred_to_role IS NULL)
);
CREATE INDEX ticket_operations_close_due ON ticket_operations(close_request_expires_at)
    WHERE close_request_expires_at IS NOT NULL;

CREATE TABLE ticket_staff_notes (
    id BIGSERIAL PRIMARY KEY,
    ticket_id BIGINT NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
    author_id BIGINT NOT NULL,
    body TEXT NOT NULL CHECK (length(body) BETWEEN 1 AND 2000),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX ticket_staff_notes_ticket ON ticket_staff_notes(ticket_id, id DESC);

CREATE TABLE ticket_snippets (
    id BIGSERIAL PRIMARY KEY,
    guild_id BIGINT NOT NULL,
    name TEXT NOT NULL,
    body TEXT NOT NULL CHECK (length(body) BETWEEN 1 AND 2000),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (guild_id, name)
);

CREATE TABLE ticket_form_drafts (
    guild_id BIGINT NOT NULL,
    user_id BIGINT NOT NULL,
    panel_id BIGINT NOT NULL REFERENCES ticket_panels(id) ON DELETE CASCADE,
    topic_index INTEGER NOT NULL,
    answers JSONB NOT NULL DEFAULT '[]',
    step INTEGER NOT NULL DEFAULT 0,
    expires_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (guild_id, user_id, panel_id, topic_index)
);
CREATE INDEX ticket_form_drafts_expiry ON ticket_form_drafts(expires_at);
