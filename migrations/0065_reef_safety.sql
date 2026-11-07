-- guestbook_blocks is now the general social block list. Existing blocks keep their meaning.
CREATE INDEX guestbook_blocks_reverse ON guestbook_blocks(blocked_id, host_id);
CREATE TABLE reef_consents (
    user_id BIGINT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    age_band TEXT NOT NULL CHECK (age_band IN ('under_13','13_plus')),
    accepted BOOLEAN NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE reef_sanctions (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL,
    actor_id BIGINT NOT NULL,
    owner_id BIGINT,
    room_idx SMALLINT CHECK (room_idx BETWEEN 0 AND 5),
    kind TEXT NOT NULL CHECK (kind IN ('mute','kick','ban')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    CHECK (owner_id IS NOT NULL OR room_idx IS NULL)
);
CREATE INDEX reef_sanctions_active ON reef_sanctions(user_id, expires_at);
CREATE TABLE reef_chat_evidence (
    id BIGSERIAL PRIMARY KEY,
    owner_id BIGINT NOT NULL,
    room_idx SMALLINT NOT NULL CHECK (room_idx BETWEEN 0 AND 5),
    sender_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    text TEXT NOT NULL CHECK (octet_length(text) <= 1600),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    reported BOOLEAN NOT NULL DEFAULT FALSE
);
CREATE INDEX reef_chat_retention ON reef_chat_evidence(created_at);
CREATE TABLE reef_chat_recipients (
    message_id BIGINT NOT NULL REFERENCES reef_chat_evidence(id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL,
    PRIMARY KEY(message_id, user_id)
);
CREATE TABLE reef_reports (
    id BIGSERIAL PRIMARY KEY,
    message_id BIGINT NOT NULL REFERENCES reef_chat_evidence(id) ON DELETE CASCADE,
    reporter_id BIGINT NOT NULL,
    reason TEXT NOT NULL CHECK (reason IN ('harassment','personal_info','spam','other')),
    status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open','resolved','appealed')),
    disposition TEXT CHECK (disposition IN ('no_action','warned','mute','kick','ban')),
    appeal TEXT CHECK (octet_length(appeal) <= 1600),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(message_id, reporter_id)
);
CREATE TABLE reef_moderator_audit (
    id BIGSERIAL PRIMARY KEY,
    actor_id BIGINT NOT NULL,
    target_user_id BIGINT,
    action TEXT NOT NULL,
    subject_id BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
