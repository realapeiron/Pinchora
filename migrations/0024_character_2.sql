-- Character 2.0: accessories bought with shells, and three saved outfits per player. Free
-- accessories and the ones opened by a level or a badge are worked out, not stored. The
-- character itself stays in users.avatar, in the longer code that now carries eyes, shoes
-- and accessories (see pinchora-game's `Character`).

CREATE TABLE user_accessories (
    user_id       BIGINT      NOT NULL,
    accessory_id  TEXT        NOT NULL,
    bought_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, accessory_id)
);

-- A saved character code under a name the player chose, in slots 1 to 3.
CREATE TABLE character_outfits (
    user_id   BIGINT      NOT NULL,
    slot      SMALLINT    NOT NULL CHECK (slot BETWEEN 1 AND 3),
    name      TEXT        NOT NULL CHECK (char_length(name) BETWEEN 1 AND 32),
    code      TEXT        NOT NULL,
    saved_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, slot)
);
