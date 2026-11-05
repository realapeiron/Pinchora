-- Calm games: duels with escrowed wagers, trivia rounds, wordle boards and win counts.

-- Win counts for /play stats. `played` counts finished games, `won` the ones the player won.
CREATE TABLE game_stats (
    user_id  BIGINT  NOT NULL,
    game     TEXT    NOT NULL,
    played   INTEGER NOT NULL DEFAULT 0,
    won      INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id, game)
);

-- One row per duel, hangman or trivia round. `state` belongs to the game that owns the row.
-- Status: `pending` (a challenge nobody accepted yet), `active`, `done`. For a duel with a bet,
-- `active` means both players' bets are held: the bet came off both balances when the challenge was
-- accepted, and leaving `active` pays the pot to the winner or gives the bets back, in the same
-- transaction as the status change, so shells are never lost or paid twice, even across a restart.
-- `expires_at` is the deadline of the current turn, round or challenge; a janitor settles late rows.
CREATE TABLE game_sessions (
    id          BIGSERIAL   PRIMARY KEY,
    kind        TEXT        NOT NULL,
    guild_id    BIGINT      NOT NULL,
    channel_id  BIGINT      NOT NULL,
    message_id  BIGINT,
    host_id     BIGINT      NOT NULL,
    guest_id    BIGINT,
    bet         BIGINT      NOT NULL DEFAULT 0 CHECK (bet >= 0),
    status      TEXT        NOT NULL DEFAULT 'pending',
    state       TEXT        NOT NULL DEFAULT '',
    expires_at  TIMESTAMPTZ NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX game_sessions_open_idx ON game_sessions (expires_at) WHERE status <> 'done';

-- Who answered a trivia round, and with which slot. One answer each.
CREATE TABLE game_answers (
    session_id  BIGINT   NOT NULL REFERENCES game_sessions (id) ON DELETE CASCADE,
    user_id     BIGINT   NOT NULL,
    choice      SMALLINT NOT NULL,
    PRIMARY KEY (session_id, user_id)
);

-- A wordle board. The daily word keeps one row per player and day (the result stays); practice
-- keeps one row per player, replaced by the next practice word.
CREATE TABLE wordle_games (
    user_id     BIGINT      NOT NULL,
    mode        TEXT        NOT NULL,
    day         INTEGER     NOT NULL DEFAULT 0,
    word        TEXT        NOT NULL,
    guesses     TEXT[]      NOT NULL DEFAULT '{}',
    status      TEXT        NOT NULL DEFAULT 'playing',
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, mode, day)
);
