-- Fishing depth: spots, rods and baits, a fish bucket, book milestones and the weekly catch board.

-- What a player has on the line. No row means the shore, the starter rod and plain bait; a row
-- appears the first time they change something. `spot` is the last one they fished, `bait` NULL means
-- plain bait.
CREATE TABLE fishing_gear (
    user_id  BIGINT PRIMARY KEY,
    spot     TEXT NOT NULL DEFAULT 'shore',
    rod      TEXT NOT NULL DEFAULT 'driftwood_rod',
    bait     TEXT
);

-- Rods bought with shells. The starter rod is everybody's and is not stored.
CREATE TABLE user_rods (
    user_id    BIGINT      NOT NULL,
    rod_id     TEXT        NOT NULL,
    bought_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, rod_id)
);

-- Bait pieces left, by kind. A cast with bait on the hook uses one up.
CREATE TABLE user_baits (
    user_id  BIGINT  NOT NULL,
    bait_id  TEXT    NOT NULL,
    qty      INTEGER NOT NULL DEFAULT 0 CHECK (qty >= 0),
    PRIMARY KEY (user_id, bait_id)
);

-- Caught fish waiting to be sold, one row per fish. The fish book keeps the species and the record
-- size for good; the bucket is only what is still to sell.
CREATE TABLE fish_bucket (
    id         BIGSERIAL   PRIMARY KEY,
    user_id    BIGINT      NOT NULL,
    fish_id    TEXT        NOT NULL,
    size_cm    REAL        NOT NULL,
    caught_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX fish_bucket_user_idx ON fish_bucket (user_id, fish_id);

-- Book milestones already paid, so each pays once per player: `species` is the book size it is for.
CREATE TABLE fish_milestones (
    user_id     BIGINT      NOT NULL,
    species     INTEGER     NOT NULL,
    claimed_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, species)
);

-- The biggest catch of each player in a server for a week, by how big it was for its species
-- (`ratio`: 1.0 is a usual size). `week` is the Monday (UTC) the week starts on, like `weekly_xp`;
-- old weeks stay for the "last week" recap and are pruned later.
CREATE TABLE weekly_catch (
    guild_id   BIGINT           NOT NULL,
    week       DATE             NOT NULL,
    user_id    BIGINT           NOT NULL,
    fish_id    TEXT             NOT NULL,
    size_cm    REAL             NOT NULL,
    ratio      DOUBLE PRECISION NOT NULL,
    caught_at  TIMESTAMPTZ      NOT NULL DEFAULT now(),
    PRIMARY KEY (guild_id, week, user_id)
);

CREATE INDEX weekly_catch_board_idx ON weekly_catch (guild_id, week, ratio DESC);
