-- Server events: giveaways, suggestions and polls.

-- A giveaway. `shells` is the prize each winner is paid. It is held from the host when the giveaway
-- starts: `winners` x `shells` came off the host's balance in the transaction that created the row,
-- so a server cannot mint shells. Ending (`running` -> `ended`) picks the winners, pays them and gives
-- the host back whatever nobody won, in one transaction with the status change, so shells are never
-- lost or paid twice, even across a restart. `announced` stays false until the channel has been told;
-- the ticker retries that part on its own. Whoever is telling the channel first claims it (`announce_claimed`,
-- good for two minutes), so two pods never post the same announcement.
CREATE TABLE giveaways (
    id                BIGSERIAL   PRIMARY KEY,
    guild_id          BIGINT      NOT NULL,
    channel_id        BIGINT      NOT NULL,
    message_id        BIGINT,
    host_id           BIGINT      NOT NULL,
    prize             TEXT        NOT NULL,
    shells            BIGINT      NOT NULL DEFAULT 0 CHECK (shells BETWEEN 0 AND 1000000),
    winners           INTEGER     NOT NULL CHECK (winners BETWEEN 1 AND 20),
    required_role_id  BIGINT,
    ends_at           TIMESTAMPTZ NOT NULL,
    status            TEXT        NOT NULL DEFAULT 'running' CHECK (status IN ('running', 'ended')),
    winner_ids        BIGINT[]    NOT NULL DEFAULT '{}',
    announced         BOOLEAN     NOT NULL DEFAULT FALSE,
    announce_claimed  TIMESTAMPTZ,
    ended_at          TIMESTAMPTZ,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX giveaways_due_idx ON giveaways (ends_at) WHERE status = 'running';
CREATE INDEX giveaways_guild_idx ON giveaways (guild_id, status);
CREATE INDEX giveaways_unannounced_idx ON giveaways (id) WHERE status = 'ended' AND NOT announced;

CREATE TABLE giveaway_entries (
    giveaway_id  BIGINT NOT NULL REFERENCES giveaways (id) ON DELETE CASCADE,
    user_id      BIGINT NOT NULL,
    PRIMARY KEY (giveaway_id, user_id)
);

-- Where `/suggest` posts, and the server's running suggestion number (each server counts from 1).
CREATE TABLE suggestion_settings (
    guild_id     BIGINT  PRIMARY KEY,
    channel_id   BIGINT,
    last_number  INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE suggestions (
    id          BIGSERIAL   PRIMARY KEY,
    guild_id    BIGINT      NOT NULL,
    number      INTEGER     NOT NULL,
    author_id   BIGINT      NOT NULL,
    body        TEXT        NOT NULL,
    channel_id  BIGINT,
    message_id  BIGINT,
    status      TEXT        NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'approved', 'denied')),
    reason      TEXT,
    decided_by  BIGINT,
    decided_at  TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (guild_id, number)
);

-- One vote per member and suggestion; `up` is false for a thumbs down.
CREATE TABLE suggestion_votes (
    suggestion_id  BIGINT  NOT NULL REFERENCES suggestions (id) ON DELETE CASCADE,
    user_id        BIGINT  NOT NULL,
    up             BOOLEAN NOT NULL,
    PRIMARY KEY (suggestion_id, user_id)
);

-- A poll of two to five options. Closing (`open` -> `closed`) happens once, on the ticker; `announced`
-- works as it does for giveaways.
CREATE TABLE polls (
    id          BIGSERIAL   PRIMARY KEY,
    guild_id    BIGINT      NOT NULL,
    channel_id  BIGINT      NOT NULL,
    message_id  BIGINT,
    author_id   BIGINT      NOT NULL,
    question    TEXT        NOT NULL,
    options     TEXT[]      NOT NULL CHECK (cardinality(options) BETWEEN 2 AND 5),
    ends_at     TIMESTAMPTZ NOT NULL,
    status      TEXT        NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'closed')),
    announced   BOOLEAN     NOT NULL DEFAULT FALSE,
    announce_claimed  TIMESTAMPTZ,
    closed_at   TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX polls_due_idx ON polls (ends_at) WHERE status = 'open';
CREATE INDEX polls_author_idx ON polls (guild_id, author_id) WHERE status = 'open';
CREATE INDEX polls_unannounced_idx ON polls (id) WHERE status = 'closed' AND NOT announced;

-- One vote per member and poll; `choice` is the option's position, from 0.
CREATE TABLE poll_votes (
    poll_id  BIGINT   NOT NULL REFERENCES polls (id) ON DELETE CASCADE,
    user_id  BIGINT   NOT NULL,
    choice   SMALLINT NOT NULL CHECK (choice BETWEEN 0 AND 4),
    PRIMARY KEY (poll_id, user_id)
);
