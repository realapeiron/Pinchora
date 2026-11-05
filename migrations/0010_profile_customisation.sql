-- Profile card backgrounds and badges. What a player owns, and what they wear.

-- Backgrounds bought with shells (see content/backgrounds.json).
CREATE TABLE user_backgrounds (
    user_id        BIGINT      NOT NULL,
    background_id  TEXT        NOT NULL,
    bought_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, background_id)
);

-- Shop badges bought with shells. Achievement badges are not stored: they are worked
-- out from the player's real progress whenever they are needed.
CREATE TABLE user_badges (
    user_id    BIGINT      NOT NULL,
    badge_id   TEXT        NOT NULL,
    bought_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, badge_id)
);

-- The background on their cards (NULL: the plain look) and the badges on their profile
-- card, in order, at most six.
ALTER TABLE users
    ADD COLUMN background TEXT,
    ADD COLUMN badges     TEXT[] NOT NULL DEFAULT '{}' CHECK (cardinality(badges) <= 6);
