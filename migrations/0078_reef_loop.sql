-- D7: durable fish/house/pet loop. All money-changing operations take lock_user first.
CREATE TABLE reef_loop (
 user_id BIGINT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
 seaweed INTEGER NOT NULL DEFAULT 0 CHECK (seaweed BETWEEN 0 AND 999),
 aquarium_name TEXT NOT NULL DEFAULT 'Ripple Nook' CHECK (char_length(aquarium_name) BETWEEN 1 AND 24),
 planted_at TIMESTAMPTZ, watered_at TIMESTAMPTZ,
 pet_day INTEGER, pet_streak INTEGER NOT NULL DEFAULT 0,
 dive_nonce BIGINT NOT NULL DEFAULT 0, dive_depth INTEGER NOT NULL DEFAULT 0 CHECK (dive_depth BETWEEN 0 AND 3),
 dive_until TIMESTAMPTZ
);
CREATE TABLE reef_aquarium (
 user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 slot SMALLINT NOT NULL CHECK (slot BETWEEN 1 AND 6),
 fish_id TEXT NOT NULL, size_cm REAL NOT NULL CHECK (size_cm > 0),
 PRIMARY KEY(user_id, slot)
);
-- Retained for the current UTC day by the privacy registry, preventing delete/rejoin farming.
CREATE TABLE reef_loop_daily (
 user_id BIGINT NOT NULL, day INTEGER NOT NULL,
 requests INTEGER NOT NULL DEFAULT 0 CHECK (requests BETWEEN 0 AND 3),
 dives INTEGER NOT NULL DEFAULT 0 CHECK (dives BETWEEN 0 AND 1),
 pet_reward BOOLEAN NOT NULL DEFAULT FALSE, partner_reward BOOLEAN NOT NULL DEFAULT FALSE,
 PRIMARY KEY(user_id, day)
);
CREATE INDEX reef_loop_daily_prune ON reef_loop_daily(day);
CREATE TABLE reef_requests (
 user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 rotation BIGINT NOT NULL, PRIMARY KEY(user_id, rotation)
);
CREATE INDEX reef_requests_prune ON reef_requests(rotation);
CREATE TABLE reef_catches (
 guild_id BIGINT NOT NULL, day DATE NOT NULL, user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 fish_id TEXT NOT NULL, size_cm REAL NOT NULL, score DOUBLE PRECISION NOT NULL CHECK (score > 0),
 caught_at TIMESTAMPTZ NOT NULL DEFAULT now(), PRIMARY KEY(guild_id, day, user_id)
);
CREATE INDEX reef_catches_board ON reef_catches(guild_id, day, score DESC, caught_at);
-- Cover user deletion/season qualification and the global retention sweep. The board's
-- guild-leading index cannot support these lookups on a populated live database.
CREATE INDEX reef_catches_user ON reef_catches(user_id, guild_id, day);
CREATE INDEX reef_catches_prune ON reef_catches(day);
CREATE TABLE reef_season_claims (
 user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 season DATE NOT NULL, PRIMARY KEY(user_id, season)
);
CREATE TABLE reef_partner_streaks (
 user_a BIGINT PRIMARY KEY REFERENCES partners(user_a) ON DELETE CASCADE,
 user_b BIGINT NOT NULL, since TIMESTAMPTZ NOT NULL,
 a_day INTEGER, b_day INTEGER, completed_day INTEGER, streak INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX reef_partner_streaks_user_b ON reef_partner_streaks(user_b);
