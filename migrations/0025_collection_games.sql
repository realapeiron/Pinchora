-- Collection games: Critter Drop, Message in a Bottle, Beachcomb with the Tide Pool Dex, and the Crab Pot.

-- Whether chat in a server makes critter drops and bottles wash up, and where. Off until a manager turns
-- it on with `/config critters`. With a channel, only chat there counts and the drops land there; without
-- one, drops appear in the channel whose message tipped the count.
CREATE TABLE collection_settings (
    guild_id    BIGINT  PRIMARY KEY,
    enabled     BOOLEAN NOT NULL DEFAULT FALSE,
    channel_id  BIGINT
);

-- The Tide Pool Dex: one row per player and entry (a critter or a find, ids from content/critters.json and
-- content/finds.json). `caught` counts every time they got it; the first is the discovery, later ones are
-- recycled for shells and fragments.
CREATE TABLE dex_entries (
    user_id   BIGINT      NOT NULL,
    entry_id  TEXT        NOT NULL,
    caught    INTEGER     NOT NULL DEFAULT 1 CHECK (caught >= 1),
    first_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, entry_id)
);

-- What a player keeps besides the dex: their fragments (what spares leave, spent on decor), when they may
-- beachcomb again, and their crab pot. `pot_ends_at` is NULL while the pot is empty; a soak is a stored end
-- time, so it survives restarts, and hauling clears it in the transaction that pays.
CREATE TABLE collection_players (
    user_id        BIGINT      PRIMARY KEY,
    fragments      BIGINT      NOT NULL DEFAULT 0 CHECK (fragments >= 0),
    comb_ready_at  TIMESTAMPTZ,
    pot_level      SMALLINT    NOT NULL DEFAULT 1 CHECK (pot_level BETWEEN 1 AND 5),
    pot_hours      SMALLINT,
    pot_ends_at    TIMESTAMPTZ
);

-- How many plays of each game a player had on a UTC day (days since 1970): only the first few a day pay
-- shells, later ones still fill the dex. Counted in the same transaction as the payment.
CREATE TABLE collection_daily (
    user_id  BIGINT   NOT NULL,
    day      INTEGER  NOT NULL,
    game     TEXT     NOT NULL,
    plays    INTEGER  NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id, day, game)
);

-- A critter drop (three cards, a button each) or a bottle (one slot, one opener). `cards` are the entry ids
-- (a bottle's is its message id) and `takers` who has each slot, 0 while free. A slot is taken inside one
-- transaction that also pays, so two clicks can never take the same card, and nobody takes two.
CREATE TABLE collection_drops (
    id          BIGSERIAL   PRIMARY KEY,
    kind        TEXT        NOT NULL CHECK (kind IN ('critters', 'bottle')),
    guild_id    BIGINT      NOT NULL,
    channel_id  BIGINT      NOT NULL,
    message_id  BIGINT,
    host_id     BIGINT,
    cards       TEXT[]      NOT NULL,
    takers      BIGINT[]    NOT NULL,
    status      TEXT        NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'closed')),
    expires_at  TIMESTAMPTZ NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX collection_drops_due_idx ON collection_drops (expires_at) WHERE status = 'open';
CREATE INDEX collection_drops_channel_idx ON collection_drops (channel_id) WHERE status = 'open';
