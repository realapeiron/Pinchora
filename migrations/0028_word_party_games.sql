-- Word and party games: Hot Pearl, Acronym Tide, Caption the Postcard, Wordle Race, Would You Rather and
-- Truth or Dare. The first four and Truth or Dare are group games with a lobby; Would You Rather is a poll.

-- Whether a server wants the strict word filter on what members type into these games. The base filter
-- (strong words and slurs) always applies; `/config partygames` is the only thing that touches this.
CREATE TABLE party_settings (
    guild_id  BIGINT  PRIMARY KEY,
    strict    BOOLEAN NOT NULL DEFAULT FALSE
);

-- One row per lobby, game or poll. `status` is where it is overall, `phase` what it is waiting for (a
-- lobby's start, a turn, written entries, votes, a reveal, a race), and `deadline` when that wait ends: the
-- ticker takes over whatever is late. `seq` goes up with every change, so a move made on a stale picture
-- of the game (two clicks, a click and the clock) is refused instead of applied twice. `state` is the
-- game's own JSON: the syllable and whose turn it is, the letters, the postcard, the secret word.
CREATE TABLE party_games (
    id          BIGSERIAL   PRIMARY KEY,
    kind        TEXT        NOT NULL
                CHECK (kind IN ('hotpearl', 'acronym', 'postcard', 'wordrace', 'truthordare', 'wouldyourrather')),
    guild_id    BIGINT      NOT NULL,
    channel_id  BIGINT      NOT NULL,
    message_id  BIGINT,
    host_id     BIGINT      NOT NULL,
    status      TEXT        NOT NULL DEFAULT 'lobby' CHECK (status IN ('lobby', 'playing', 'done')),
    phase       TEXT        NOT NULL DEFAULT 'lobby',
    round       INTEGER     NOT NULL DEFAULT 0,
    rounds      INTEGER     NOT NULL DEFAULT 1,
    seq         INTEGER     NOT NULL DEFAULT 0,
    state       TEXT        NOT NULL DEFAULT '{}',
    deadline    TIMESTAMPTZ NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX party_games_due_idx ON party_games (deadline) WHERE status <> 'done';
CREATE INDEX party_games_channel_idx ON party_games (channel_id) WHERE status <> 'done';

-- Who is in a game. `seat` is the order they joined in (a game that needs another order rewrites it when it
-- starts), `lives` and `score` are the game's own counters, `eliminated` marks a player who has lost their
-- last life, and `guesses` is a Wordle Race board.
CREATE TABLE party_players (
    game_id     BIGINT   NOT NULL REFERENCES party_games (id) ON DELETE CASCADE,
    user_id     BIGINT   NOT NULL,
    name        TEXT     NOT NULL,
    seat        INTEGER  NOT NULL,
    lives       INTEGER  NOT NULL DEFAULT 0,
    score       INTEGER  NOT NULL DEFAULT 0,
    eliminated  BOOLEAN  NOT NULL DEFAULT FALSE,
    guesses     TEXT[]   NOT NULL DEFAULT '{}',
    PRIMARY KEY (game_id, user_id)
);

-- What players typed: a word of Hot Pearl (`round` is the turn), a phrase of Acronym Tide or a caption of
-- Caption the Postcard (one per player and round, a later one replaces an earlier one).
CREATE TABLE party_entries (
    game_id  BIGINT  NOT NULL REFERENCES party_games (id) ON DELETE CASCADE,
    round    INTEGER NOT NULL,
    user_id  BIGINT  NOT NULL,
    text     TEXT    NOT NULL,
    PRIMARY KEY (game_id, round, user_id)
);

-- Votes: for the author of an entry (`pick` is a user id) in the written games, for side 0 or 1 in a
-- Would You Rather poll. One vote per voter and round; changing it is an update.
CREATE TABLE party_votes (
    game_id   BIGINT   NOT NULL REFERENCES party_games (id) ON DELETE CASCADE,
    round     INTEGER  NOT NULL,
    voter_id  BIGINT   NOT NULL,
    pick      BIGINT   NOT NULL,
    PRIMARY KEY (game_id, round, voter_id)
);

-- How many games of each kind paid a player on a UTC day (days since 1970): only the first few a day pay
-- shells. Counted in the same transaction as the payment.
CREATE TABLE party_daily (
    user_id  BIGINT   NOT NULL,
    day      INTEGER  NOT NULL,
    game     TEXT     NOT NULL,
    plays    INTEGER  NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id, day, game)
);
