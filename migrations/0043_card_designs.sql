-- Welcome and goodbye images a server designs on the dashboard (`pinchora_core::card_design`), and how
-- they are sent. Without a row, or with `design` NULL, the bot draws the classic card from the `guilds`
-- welcome columns and sends it the way it always did.
CREATE TABLE card_designs (
    guild_id    BIGINT      NOT NULL,
    kind        TEXT        NOT NULL CHECK (kind IN ('welcome', 'goodbye')),
    design      JSONB,
    delivery    TEXT        NOT NULL
                CHECK (delivery IN ('with_message', 'before_message', 'image_only', 'text_only', 'dm')),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (guild_id, kind)
);

-- Background pictures uploaded for those designs, re-encoded as PNG and shrunk to fit the largest canvas.
CREATE TABLE card_uploads (
    id          TEXT        PRIMARY KEY CHECK (id ~ '^[0-9a-f]{32}$'),
    guild_id    BIGINT      NOT NULL,
    png         BYTEA       NOT NULL,
    width       INT         NOT NULL,
    height      INT         NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX card_uploads_guild_idx ON card_uploads (guild_id, created_at DESC);
