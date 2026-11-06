-- The hourly prune of old weekly XP deletes by week alone. The primary key and the board index both
-- start with guild_id, so neither can serve `WHERE week < $1`: without this it scans the whole table.
CREATE INDEX weekly_xp_week_idx ON weekly_xp (week);
