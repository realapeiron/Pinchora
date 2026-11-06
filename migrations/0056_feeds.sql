-- Social feeds (phase 1: YouTube, RSS and podcasts, Reddit, Bluesky, and an operator's RSS bridge).
--
-- A source is fetched once however many servers follow it: `feed_sources` holds what the poller knows
-- about it (conditional-request validators, when to look next, how it failed), `feeds` the servers'
-- subscriptions with their channel and message. `feed_items_seen` remembers what was already announced
-- so an item is posted once, whichever pod polls; `feed_outbox` holds the posts still to send, one per
-- subscription and item, so a pod that dies half-way leaves the rest for another.

CREATE TABLE feed_sources (
    kind            TEXT        NOT NULL,
    source          TEXT        NOT NULL,
    -- What the source calls itself (the channel's name, the feed's title).
    title           TEXT,
    etag            TEXT,
    last_modified   TEXT,
    next_poll_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    interval_secs   INT         NOT NULL DEFAULT 300 CHECK (interval_secs BETWEEN 60 AND 86400),
    fail_count      INT         NOT NULL DEFAULT 0,
    last_error      TEXT,
    last_checked_at TIMESTAMPTZ,
    -- When the newest item the source showed was published.
    last_item_at    TIMESTAMPTZ,
    -- Until a first fetch has recorded what is already there, nothing is announced.
    primed          BOOLEAN     NOT NULL DEFAULT FALSE,
    lease_until     TIMESTAMPTZ,
    -- Enabled subscriptions; a source nobody follows is not polled and is forgotten after a day.
    subscribers     INT         NOT NULL DEFAULT 0,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (kind, source)
);
CREATE INDEX feed_sources_due ON feed_sources (next_poll_at) WHERE subscribers > 0;

CREATE TABLE feeds (
    id                BIGSERIAL   PRIMARY KEY,
    guild_id          BIGINT      NOT NULL,
    kind              TEXT        NOT NULL,
    source            TEXT        NOT NULL,
    display_name      TEXT,
    channel_id        BIGINT      NOT NULL,
    -- NULL: the default message for the kind, in the server's language.
    message           TEXT        CHECK (char_length(message) <= 500),
    embed             BOOLEAN     NOT NULL DEFAULT TRUE,
    ping_role_id      BIGINT,
    filter_include    TEXT[]      NOT NULL DEFAULT '{}' CHECK (cardinality(filter_include) <= 10),
    filter_exclude    TEXT[]      NOT NULL DEFAULT '{}' CHECK (cardinality(filter_exclude) <= 10),
    enabled           BOOLEAN     NOT NULL DEFAULT TRUE,
    last_item_id      TEXT,
    last_sent_at      TIMESTAMPTZ,
    -- Why the last post did not go out (no permission in the channel, the channel is gone).
    last_error        TEXT,
    delivery_failures INT         NOT NULL DEFAULT 0,
    created_by        BIGINT      NOT NULL DEFAULT 0,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (guild_id, kind, source, channel_id),
    FOREIGN KEY (kind, source) REFERENCES feed_sources (kind, source)
);
CREATE INDEX feeds_by_source ON feeds (kind, source);
CREATE INDEX feeds_by_guild ON feeds (guild_id, id);

CREATE TABLE feed_items_seen (
    kind    TEXT        NOT NULL,
    source  TEXT        NOT NULL,
    item_id TEXT        NOT NULL,
    -- Refreshed whenever a fetch still shows the item, so only items that left the feed age out.
    seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (kind, source, item_id)
);
CREATE INDEX feed_items_seen_age ON feed_items_seen (seen_at);

CREATE TABLE feed_outbox (
    feed_id     BIGINT      NOT NULL REFERENCES feeds (id) ON DELETE CASCADE,
    item_id     TEXT        NOT NULL,
    -- The item as the post needs it (title, link, author, picture, published).
    item        JSONB       NOT NULL,
    attempts    INT         NOT NULL DEFAULT 0,
    lease_until TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (feed_id, item_id)
);
CREATE INDEX feed_outbox_due ON feed_outbox (created_at);
