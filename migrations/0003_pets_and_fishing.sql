-- Needs are stored as of `needs_at` and decay on read, so no background job is needed.
CREATE TABLE pets (
    id          BIGSERIAL PRIMARY KEY,
    owner_id    BIGINT      NOT NULL,
    species     TEXT        NOT NULL,
    name        TEXT        NOT NULL,
    level       INTEGER     NOT NULL DEFAULT 1,
    xp          INTEGER     NOT NULL DEFAULT 0,
    hunger      SMALLINT    NOT NULL DEFAULT 80,
    fun         SMALLINT    NOT NULL DEFAULT 80,
    energy      SMALLINT    NOT NULL DEFAULT 80,
    hygiene     SMALLINT    NOT NULL DEFAULT 80,
    needs_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    active      BOOLEAN     NOT NULL DEFAULT FALSE,
    adopted_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX pets_owner_idx ON pets (owner_id);
CREATE UNIQUE INDEX pets_one_active_idx ON pets (owner_id) WHERE active;

CREATE TABLE fish_book (
    user_id     BIGINT      NOT NULL,
    fish_id     TEXT        NOT NULL,
    caught      INTEGER     NOT NULL DEFAULT 0,
    best_cm     REAL        NOT NULL DEFAULT 0,
    first_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, fish_id)
);
