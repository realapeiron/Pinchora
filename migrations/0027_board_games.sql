-- Board and puzzle games: Sea Fleet, Pixel Picross, Tide 2048, Pearl Checkers and Reef Chess. The
-- two-player ones (Sea Fleet, Pearl Checkers, Reef Chess) keep their whole state in game_sessions, like
-- connect four does (see 0014); this file adds what the solo ones need.

-- A best score next to the win counts (Tide 2048 keeps its highest score here).
ALTER TABLE game_stats ADD COLUMN best INTEGER NOT NULL DEFAULT 0;

-- One pixel picross board per player, mode and day. The daily puzzle keeps a row per player and day
-- (the result stays, and the streak is counted from the days solved); practice keeps one row per player
-- (day 0), replaced by the next puzzle. `grid` holds the player's marks, 100 characters of `#` and `.`.
-- The step from `playing` to `solved` happens once, in one update, and the shells are paid by whoever
-- made it. `checks` counts the times the player asked how many of their marks are wrong; a puzzle that
-- needed one pays less.
CREATE TABLE picross_games (
    user_id     BIGINT      NOT NULL,
    mode        TEXT        NOT NULL,
    day         INTEGER     NOT NULL DEFAULT 0,
    picture     SMALLINT    NOT NULL,
    variant     SMALLINT    NOT NULL,
    grid        TEXT        NOT NULL,
    checks      SMALLINT    NOT NULL DEFAULT 0,
    status      TEXT        NOT NULL DEFAULT 'playing',
    started_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    solved_at   TIMESTAMPTZ,
    PRIMARY KEY (user_id, mode, day)
);

-- A solo game in progress, one row per player and game (Tide 2048): its board in `state`, the points so
-- far and the moves made. `status` goes from `playing` to `over` once, in one update.
CREATE TABLE solo_games (
    user_id     BIGINT      NOT NULL,
    game        TEXT        NOT NULL,
    state       TEXT        NOT NULL,
    score       INTEGER     NOT NULL DEFAULT 0,
    moves       INTEGER     NOT NULL DEFAULT 0,
    status      TEXT        NOT NULL DEFAULT 'playing',
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, game)
);

-- The Sea Fleet layout a captain is looking at before they lock it in. The layout stays here, on the server;
-- the buttons on the private message carry only `rev`, a counter that goes up with every new layout, so a
-- click on an older message cannot lock in a layout the captain is not looking at. The row goes when the
-- layout is locked in (into the game's state) or when the game goes.
CREATE TABLE fleet_drafts (
    session_id  BIGINT      NOT NULL REFERENCES game_sessions (id) ON DELETE CASCADE,
    user_id     BIGINT      NOT NULL,
    layout      TEXT        NOT NULL,
    rev         BIGINT      NOT NULL DEFAULT 1,
    PRIMARY KEY (session_id, user_id)
);
