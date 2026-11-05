-- A player's pixel character, saved as "<look>.<skin>.<hair>.<top>.<bottom>"
-- (see pinchora-game's `Character`). NULL until they design one with /character.
ALTER TABLE users ADD COLUMN avatar TEXT;
