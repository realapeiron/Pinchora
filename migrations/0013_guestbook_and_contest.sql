-- Notes visitors leave in a house's guestbook after `/house visit`: one per visitor, host and day.
CREATE TABLE guestbook_notes (
    id           BIGSERIAL PRIMARY KEY,
    host_id      BIGINT      NOT NULL,
    author_id    BIGINT      NOT NULL,
    -- The signer's name when they signed, so the guestbook never has to look it up.
    author_name  TEXT        NOT NULL,
    day          DATE        NOT NULL,
    body         TEXT        NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (host_id, author_id, day)
);

CREATE INDEX guestbook_notes_host_idx ON guestbook_notes (host_id, id DESC);

-- Signers a host blocked: they cannot sign that guestbook again.
CREATE TABLE guestbook_blocks (
    host_id       BIGINT      NOT NULL,
    blocked_id    BIGINT      NOT NULL,
    blocked_name  TEXT        NOT NULL,
    blocked_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (host_id, blocked_id)
);

-- The newest note the host has read, for "3 new guestbook notes".
ALTER TABLE houses ADD COLUMN guestbook_seen BIGINT NOT NULL DEFAULT 0;

-- The weekly room contest, per server. `week` counts weeks from Monday 1970-01-05 00:00 UTC.
CREATE TABLE contest_entries (
    guild_id    BIGINT      NOT NULL,
    week        INTEGER     NOT NULL,
    user_id     BIGINT      NOT NULL,
    owner_name  TEXT        NOT NULL,
    room_idx    SMALLINT    NOT NULL,
    -- The house tier sets the room size the snapshot is drawn at.
    tier        SMALLINT    NOT NULL,
    room_type   TEXT        NOT NULL,
    wallpaper   TEXT        NOT NULL,
    floor       TEXT        NOT NULL,
    entered_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (guild_id, week, user_id)
);

-- An entry's furniture as it stood when the room was entered.
CREATE TABLE contest_pieces (
    guild_id  BIGINT   NOT NULL,
    week      INTEGER  NOT NULL,
    user_id   BIGINT   NOT NULL,
    n         INTEGER  NOT NULL,
    item_id   TEXT     NOT NULL,
    x         SMALLINT NOT NULL,
    y         SMALLINT NOT NULL,
    flipped   BOOLEAN  NOT NULL,
    PRIMARY KEY (guild_id, week, user_id, n),
    FOREIGN KEY (guild_id, week, user_id) REFERENCES contest_entries ON DELETE CASCADE
);

-- One vote per entry per voter; `entry_id` is the entrant's user id.
CREATE TABLE contest_votes (
    guild_id  BIGINT      NOT NULL,
    week      INTEGER     NOT NULL,
    entry_id  BIGINT      NOT NULL,
    voter_id  BIGINT      NOT NULL,
    voted_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (guild_id, week, entry_id, voter_id),
    FOREIGN KEY (guild_id, week, entry_id) REFERENCES contest_entries ON DELETE CASCADE
);

CREATE INDEX contest_votes_voter_idx ON contest_votes (guild_id, week, voter_id);

-- A finished week, written once (with the prizes) by whichever call closes it first.
CREATE TABLE contest_results (
    guild_id      BIGINT      NOT NULL,
    week          INTEGER     NOT NULL,
    -- First, second and third place; fewer when fewer entries got a vote.
    winner_ids    BIGINT[]    NOT NULL,
    winner_votes  INTEGER[]   NOT NULL,
    entries       INTEGER     NOT NULL,
    finalized_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (guild_id, week)
);

-- Titles players earn. Another migration creates the same table, so whichever runs first makes it.
CREATE TABLE IF NOT EXISTS user_titles (
    user_id     BIGINT      NOT NULL,
    title_id    TEXT        NOT NULL,
    granted_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, title_id)
);
