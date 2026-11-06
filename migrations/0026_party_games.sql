-- Party games: the hermit crab derby, share or grab, the shell game, the treasure heist and the
-- storm surge. Lobbies and running games are rows of `game_sessions` (kinds `derby`, `sharegrab`,
-- `shellgame`, `heist` and `surge`); what is specific to them lives here.

-- The per-server switch for gambling-style games (the shell game). Off until a manager turns it on.
-- `IF NOT EXISTS`: other games that need the same switch can add it in their own migration.
ALTER TABLE guilds ADD COLUMN IF NOT EXISTS gambling_enabled BOOLEAN NOT NULL DEFAULT FALSE;

-- Who is in a lobby or an event: derby racers, heist crews and storm surge helpers.
-- `choice` is the crab (derby) or the role (heist). `amount` is the stake held for a derby racer
-- and the sandbags a surge helper stacked. `name` is shown on the cards, kept so a card needs no
-- lookups. Rows go with their session.
CREATE TABLE party_members (
    session_id  BIGINT      NOT NULL REFERENCES game_sessions (id) ON DELETE CASCADE,
    user_id     BIGINT      NOT NULL,
    choice      SMALLINT    NOT NULL DEFAULT 0,
    amount      BIGINT      NOT NULL DEFAULT 0 CHECK (amount >= 0),
    name        TEXT        NOT NULL DEFAULT '',
    joined_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (session_id, user_id)
);

-- A server's storm surge settings: whether timed storms are on, where they are announced and how
-- often. `surge_next_at` is when the next one is due; the pod that moves it forward starts the storm.
CREATE TABLE party_settings (
    guild_id          BIGINT      PRIMARY KEY,
    surge_enabled     BOOLEAN     NOT NULL DEFAULT FALSE,
    surge_channel_id  BIGINT,
    surge_every_hours INTEGER     NOT NULL DEFAULT 24 CHECK (surge_every_hours BETWEEN 1 AND 336),
    surge_next_at     TIMESTAMPTZ
);

CREATE INDEX party_settings_due_idx ON party_settings (surge_next_at) WHERE surge_enabled;

-- What a player did today (UTC day since 1970-01-01): games started, shells won less shells lost at
-- the gambling table, and shells the system paid for a capped game.
CREATE TABLE party_daily (
    user_id  BIGINT  NOT NULL,
    day      INTEGER NOT NULL,
    game     TEXT    NOT NULL,
    plays    INTEGER NOT NULL DEFAULT 0,
    net      BIGINT  NOT NULL DEFAULT 0,
    paid     BIGINT  NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id, day, game)
);

-- One derby or heist lobby per channel at a time, and one storm per server.
CREATE UNIQUE INDEX party_derby_channel_idx ON game_sessions (channel_id) WHERE kind = 'derby' AND status <> 'done';
CREATE UNIQUE INDEX party_heist_channel_idx ON game_sessions (channel_id) WHERE kind = 'heist' AND status <> 'done';
CREATE UNIQUE INDEX party_surge_guild_idx ON game_sessions (guild_id) WHERE kind = 'surge' AND status <> 'done';
