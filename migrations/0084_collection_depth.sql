-- Collection depth: held duplicates, glimmer discoveries, wishlists and guarded claims.
ALTER TABLE dex_entries
    ADD COLUMN kept INTEGER NOT NULL DEFAULT 1 CHECK (kept >= 0),
    ADD COLUMN glimmer INTEGER NOT NULL DEFAULT 0 CHECK (glimmer >= 0);

ALTER TABLE collection_players
    ADD COLUMN shards BIGINT NOT NULL DEFAULT 0 CHECK (shards >= 0);

CREATE TABLE collection_wishlist (
    user_id BIGINT NOT NULL,
    entry_id TEXT NOT NULL,
    ping_on_spawn BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, entry_id)
);
CREATE INDEX collection_wishlist_entry_ping_idx ON collection_wishlist (entry_id, created_at)
    WHERE ping_on_spawn;

CREATE TABLE collection_rolls (
    user_id BIGINT PRIMARY KEY,
    entry_id TEXT NOT NULL,
    glimmer BOOLEAN NOT NULL DEFAULT FALSE,
    rolled_at TIMESTAMPTZ NOT NULL,
    claimed_at TIMESTAMPTZ
);

CREATE TABLE collection_votes (
    drop_id BIGINT NOT NULL REFERENCES collection_drops(id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL,
    slot SMALLINT NOT NULL CHECK (slot BETWEEN 0 AND 2),
    PRIMARY KEY (drop_id, user_id)
);

ALTER TABLE collection_drops
    ADD COLUMN mode TEXT NOT NULL DEFAULT 'standard' CHECK (mode IN ('standard', 'hidden', 'word', 'vote')),
    ADD COLUMN challenge_word TEXT,
    ADD COLUMN vote_winner SMALLINT CHECK (vote_winner BETWEEN 0 AND 2),
    ADD COLUMN protected_user_id BIGINT,
    ADD COLUMN protected_slot SMALLINT,
    ADD COLUMN protected_until TIMESTAMPTZ;
