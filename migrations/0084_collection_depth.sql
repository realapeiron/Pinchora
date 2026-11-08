-- Fail quickly rather than queue an exclusive lock on a busy live table.
SET LOCAL lock_timeout = '5s';
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

-- Durable attempt budgets and an idempotent, auditable shard journal.
CREATE TABLE collection_word_attempts (
    drop_id BIGINT NOT NULL REFERENCES collection_drops(id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL,
    attempts SMALLINT NOT NULL CHECK (attempts BETWEEN 1 AND 3),
    PRIMARY KEY (drop_id, user_id)
);
CREATE TABLE collection_shard_ledger (
    user_id BIGINT NOT NULL,
    request_id TEXT NOT NULL CHECK (length(request_id) BETWEEN 1 AND 100),
    kind TEXT NOT NULL CHECK (kind IN ('recycle', 'craft')),
    entry_id TEXT NOT NULL,
    delta BIGINT NOT NULL CHECK (delta <> 0),
    balance BIGINT NOT NULL CHECK (balance >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, request_id)
);
CREATE INDEX collection_protection_idx ON collection_drops (protected_user_id, protected_until)
    WHERE protected_user_id IS NOT NULL;

-- Keep only short abuse-prevention expiries when a player's collection is erased.
ALTER TABLE deleted_users
    ADD COLUMN collection_roll_until TIMESTAMPTZ,
    ADD COLUMN collection_ping_until TIMESTAMPTZ;
