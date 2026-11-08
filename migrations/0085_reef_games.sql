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
-- Block ledger writers only for the cutover, so a pre-migration transaction cannot
-- commit a reward between the snapshot and installing the bridge. Bound lock waits.
SET LOCAL lock_timeout = '5s';
SET LOCAL statement_timeout = '30s';
LOCK TABLE credit_ledger IN SHARE ROW EXCLUSIVE MODE;
INSERT INTO game_reward_daily(user_id,day,game,wins)
 SELECT user_id, floor(extract(epoch FROM created_at)/86400)::int,
 CASE WHEN reason IN ('trivia','trivia_win') THEN 'trivia'
      ELSE regexp_replace(reason,'_win$','') END AS game, count(*)
 FROM credit_ledger
 WHERE reason IN ('chess_win','checkers_win','seafleet_win','picross_win','trivia','trivia_win')
   AND delta > 0 AND created_at >= date_trunc('day', now() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'
 GROUP BY user_id, 2, 3;

-- Old bot processes can still write ledger rewards during a rolling deployment.
-- Count and cap those writes in the same transaction as the wallet, as for new callers.
CREATE FUNCTION count_game_reward() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
 game_key text;
 reward_cap bigint;
 paid_wins bigint;
 reward_day integer := floor(extract(epoch FROM now())/86400)::int;
BEGIN
 IF NEW.delta <= 0 OR NEW.reason NOT IN
    ('chess_win','checkers_win','seafleet_win','picross_win','trivia','trivia_win') THEN
   RETURN NEW;
 END IF;
 game_key := CASE WHEN NEW.reason IN ('trivia','trivia_win') THEN 'trivia'
                  ELSE regexp_replace(NEW.reason,'_win$','') END;
 reward_cap := CASE WHEN game_key = 'trivia' THEN 12 ELSE 5 END;
 -- Wallet writers already hold this lock. Direct ledger writes follow the same discipline.
 PERFORM lock_user(NEW.user_id);
 INSERT INTO game_reward_daily(user_id,day,game,wins)
 VALUES(NEW.user_id,reward_day,game_key,
   CASE WHEN EXISTS(SELECT 1 FROM deleted_users WHERE user_id=NEW.user_id
     AND floor(extract(epoch FROM deleted_at)/86400)::int=reward_day)
   THEN reward_cap+1 ELSE 1 END)
 ON CONFLICT(user_id,day,game) DO UPDATE SET wins=game_reward_daily.wins+1
 RETURNING wins INTO paid_wins;
 IF paid_wins > reward_cap THEN
   RAISE EXCEPTION 'daily game reward limit' USING ERRCODE = '23514';
 END IF;
 RETURN NEW;
END;
$$;
CREATE TRIGGER credit_ledger_game_reward BEFORE INSERT ON credit_ledger
 FOR EACH ROW EXECUTE FUNCTION count_game_reward();
CREATE INDEX reef_games_due ON reef_games(deadline) WHERE NOT finished;
CREATE INDEX reef_game_players_user ON reef_game_players(user_id,game_id);
ALTER TABLE picross_games ADD COLUMN revision BIGINT NOT NULL DEFAULT 0;
-- Recreated personal puzzles must never reuse a revision held by an old browser.
-- Existing rows can stay at zero; every future insert/edit receives a fresh sequence value.
CREATE SEQUENCE picross_revision;
ALTER TABLE picross_games ALTER COLUMN revision SET DEFAULT nextval('picross_revision');
ALTER TABLE picross_games ADD COLUMN paid BIGINT NOT NULL DEFAULT 0;
