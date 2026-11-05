-- Pet depth: field trips and the daily free wake-up.

-- A pet on a field trip: where it went (an id from content/pet_trips.json) and when it is back.
-- NULL means it is at home. The row stays set after the end time until the player claims the
-- souvenir, so a claim is a single guarded UPDATE and cannot pay twice. The needs of a pet on a
-- trip are frozen: its `needs_at` is the end time, and needs only drift from there on.
ALTER TABLE pets
    ADD COLUMN trip_dest    TEXT,
    ADD COLUMN trip_ends_at TIMESTAMPTZ,
    ADD CONSTRAINT pets_trip_pair CHECK ((trip_dest IS NULL) = (trip_ends_at IS NULL));

-- The UTC day (days since 1970-01-01) of the player's last free wake-up: one a day.
ALTER TABLE users ADD COLUMN pet_wake_day INTEGER;
