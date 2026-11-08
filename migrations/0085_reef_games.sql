-- Room games have their own transport context; no fabricated Discord snowflakes.
CREATE TABLE reef_games (
 id BIGSERIAL PRIMARY KEY,
 room_key TEXT NOT NULL CHECK (length(room_key) <= 96),
 owner_id BIGINT NOT NULL,
 host_id BIGINT NOT NULL,
 kind TEXT NOT NULL CHECK (kind IN ('checkers','chess','trivia')),
 placement_id BIGINT,
 layout_revision BIGINT,
 request_id TEXT NOT NULL CHECK (length(request_id) BETWEEN 1 AND 64),
 state JSONB NOT NULL,
 revision BIGINT NOT NULL DEFAULT 0,
 finished BOOLEAN NOT NULL DEFAULT false,
 deadline TIMESTAMPTZ NOT NULL,
 created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 UNIQUE(host_id,request_id)
);
CREATE INDEX reef_games_room ON reef_games(room_key,created_at DESC);
CREATE UNIQUE INDEX reef_games_table ON reef_games(room_key,placement_id) WHERE NOT finished AND placement_id IS NOT NULL;
CREATE TABLE reef_game_players (
 game_id BIGINT NOT NULL REFERENCES reef_games(id) ON DELETE CASCADE,
 user_id BIGINT NOT NULL,
 side SMALLINT NOT NULL CHECK(side BETWEEN 0 AND 31),
 answer SMALLINT CHECK(answer BETWEEN 0 AND 3),
 paid BIGINT NOT NULL DEFAULT 0,
 PRIMARY KEY(game_id,user_id), UNIQUE(game_id,side)
);
CREATE TABLE reef_game_actions (
 game_id BIGINT NOT NULL REFERENCES reef_games(id) ON DELETE CASCADE,
 user_id BIGINT NOT NULL,
 request_id TEXT NOT NULL CHECK(length(request_id) BETWEEN 1 AND 64),
 payload JSONB NOT NULL,
 PRIMARY KEY(game_id,user_id,request_id)
);
-- Counts paid wins shared by Discord and web; today's rows survive privacy deletion.
CREATE TABLE game_reward_daily (
 user_id BIGINT NOT NULL, day INTEGER NOT NULL, game TEXT NOT NULL,
 wins BIGINT NOT NULL DEFAULT 0, PRIMARY KEY(user_id,day,game)
);
-- Existing ledger entries seed the cutover count, preserving today's paid bot wins.
INSERT INTO game_reward_daily(user_id,day,game,wins)
 SELECT user_id, floor(extract(epoch FROM created_at)/86400)::int,
 regexp_replace(reason,'_win$',''), count(*) FROM credit_ledger
 WHERE (reason LIKE '%\_win' OR reason='trivia') AND delta > 0
 GROUP BY user_id, floor(extract(epoch FROM created_at)/86400)::int, reason;
ALTER TABLE picross_games ADD COLUMN revision BIGINT NOT NULL DEFAULT 0;
ALTER TABLE picross_games ADD COLUMN paid BIGINT NOT NULL DEFAULT 0;
