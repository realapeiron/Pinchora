-- Card style and the profile showcase.

-- The card style, for the profile, rank and wallet cards: an accent colour as `#rrggbb` (NULL: the
-- background's, else the reef coral), how see-through the panels are (0 light, 1 normal, 2 solid)
-- and the frame around the card (plain, rope, coral, pearl, gold, neon).
ALTER TABLE users
    ADD COLUMN accent        TEXT CHECK (accent IS NULL OR accent ~ '^#[0-9a-f]{6}$'),
    ADD COLUMN panel_opacity SMALLINT NOT NULL DEFAULT 1 CHECK (panel_opacity BETWEEN 0 AND 2),
    ADD COLUMN frame         TEXT NOT NULL DEFAULT 'plain';

-- The showcase: up to three things on the profile card, plus a thumbnail of the house. A favourite
-- fish (from the fish book), a pet (its id), a furniture item (an owned or placed item id).
-- NULL / FALSE draws nothing, so a card with no choices looks as it always did.
ALTER TABLE users
    ADD COLUMN showcase_fish  TEXT,
    ADD COLUMN showcase_pet   BIGINT,
    ADD COLUMN showcase_item  TEXT,
    ADD COLUMN showcase_house BOOLEAN NOT NULL DEFAULT FALSE;
