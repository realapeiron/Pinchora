ALTER TABLE users ADD COLUMN pearls BIGINT NOT NULL DEFAULT 0 CHECK (pearls >= 0);

-- Items a player owns but has not placed. Placed items live in `placements`.
CREATE TABLE inventory (
    user_id      BIGINT      NOT NULL,
    item_id      TEXT        NOT NULL,
    quantity     INTEGER     NOT NULL CHECK (quantity >= 0),
    acquired_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, item_id)
);

CREATE TABLE houses (
    user_id     BIGINT PRIMARY KEY,
    tier        SMALLINT    NOT NULL DEFAULT 1 CHECK (tier BETWEEN 1 AND 6),
    name        TEXT        NOT NULL DEFAULT '',
    locked      BOOLEAN     NOT NULL DEFAULT FALSE,
    likes       INTEGER     NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE rooms (
    user_id    BIGINT   NOT NULL REFERENCES houses (user_id) ON DELETE CASCADE,
    idx        SMALLINT NOT NULL,
    room_type  TEXT     NOT NULL,
    wallpaper  TEXT     NOT NULL,
    floor      TEXT     NOT NULL,
    PRIMARY KEY (user_id, idx)
);

CREATE TABLE placements (
    id         BIGSERIAL PRIMARY KEY,
    user_id    BIGINT      NOT NULL,
    room_idx   SMALLINT    NOT NULL,
    item_id    TEXT        NOT NULL,
    x          SMALLINT    NOT NULL,
    y          SMALLINT    NOT NULL,
    flipped    BOOLEAN     NOT NULL DEFAULT FALSE,
    placed_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    FOREIGN KEY (user_id, room_idx) REFERENCES rooms (user_id, idx) ON DELETE CASCADE
);

CREATE INDEX placements_room_idx ON placements (user_id, room_idx);

-- Hourly market: rows are created lazily the first time an hour's market is opened.
CREATE TABLE market_stock (
    hour     BIGINT  NOT NULL,
    item_id  TEXT    NOT NULL,
    price    BIGINT  NOT NULL,
    stock    INTEGER NOT NULL,
    sold     INTEGER NOT NULL DEFAULT 0 CHECK (sold <= stock),
    PRIMARY KEY (hour, item_id)
);

CREATE TABLE visits (
    host_id     BIGINT      NOT NULL,
    guest_id    BIGINT      NOT NULL,
    day         DATE        NOT NULL,
    visited_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (host_id, guest_id, day)
);

CREATE INDEX visits_guest_day_idx ON visits (guest_id, day);
