-- Live feeds extend the existing subscriptions and source poller.
ALTER TABLE feeds ADD COLUMN live_role_id BIGINT, ADD COLUMN live_message_id BIGINT;
ALTER TABLE feed_sources ADD COLUMN live_stream_id TEXT, ADD COLUMN broadcaster_id TEXT,
    ADD COLUMN eventsub_registered_at TIMESTAMPTZ;

-- A member proves control by placing a short code in the public stream title or channel bio.
CREATE TABLE feed_channel_links (
    user_id BIGINT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('twitch', 'kick')),
    source TEXT NOT NULL,
    verification_code TEXT,
    code_expires_at TIMESTAMPTZ,
    verified_at TIMESTAMPTZ,
    PRIMARY KEY (user_id, kind),
    UNIQUE (kind, source)
);
CREATE INDEX feed_channel_links_source ON feed_channel_links (kind, source) WHERE verified_at IS NOT NULL;

-- EventSub retries and multi-pod delivery must share a durable dedupe key.
CREATE TABLE feed_webhook_messages (
    message_id TEXT PRIMARY KEY,
    received_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
