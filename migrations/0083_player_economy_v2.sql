-- D8: global shells are never controlled by guild administrators.
CREATE TABLE economy_players (
 user_id BIGINT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
 job TEXT NOT NULL DEFAULT 'kelp_keeper' CHECK(job IN ('kelp_keeper','coral_courier','beacon_tender')),
 shifts INTEGER NOT NULL DEFAULT 0 CHECK(shifts >= 0),
 prestige INTEGER NOT NULL DEFAULT 0 CHECK(prestige BETWEEN 0 AND 100),
 chests INTEGER NOT NULL DEFAULT 0 CHECK(chests BETWEEN 0 AND 10),
 provision_at TIMESTAMPTZ,
 provision_claimed INTEGER NOT NULL DEFAULT 0 CHECK(provision_claimed BETWEEN 0 AND 120),
 shift_token BIGINT NOT NULL DEFAULT 0,
 shift_step INTEGER NOT NULL DEFAULT 0 CHECK(shift_step BETWEEN 0 AND 3),
 shift_started TIMESTAMPTZ,
 season INTEGER NOT NULL DEFAULT 0,
 season_xp INTEGER NOT NULL DEFAULT 0 CHECK(season_xp BETWEEN 0 AND 1000)
);
-- Retained through the applicable day/month on privacy deletion; no FK deliberately.
CREATE TABLE economy_limits (
 user_id BIGINT NOT NULL, day INTEGER NOT NULL, shifts INTEGER NOT NULL DEFAULT 0,
 last_shift TIMESTAMPTZ, season_claimed INTEGER NOT NULL DEFAULT 0,
 PRIMARY KEY(user_id, day)
);
CREATE TABLE economy_requests (
 user_id BIGINT NOT NULL, request_id TEXT NOT NULL CHECK(length(request_id) BETWEEN 1 AND 100),
 result JSONB NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT now(), PRIMARY KEY(user_id, request_id)
);
CREATE SEQUENCE economy_shift_tokens;
CREATE TABLE economy_auctions (
 id BIGSERIAL PRIMARY KEY, seller_id BIGINT NOT NULL, item_id TEXT NOT NULL,
 start_price BIGINT NOT NULL CHECK(start_price BETWEEN 1 AND 10000000),
 bid BIGINT NOT NULL DEFAULT 0 CHECK(bid BETWEEN 0 AND 10000000), bidder_id BIGINT,
 ends_at TIMESTAMPTZ NOT NULL, hard_end TIMESTAMPTZ NOT NULL,
 status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','sold','expired','cancelled')),
 CHECK(bidder_id IS NULL OR bidder_id <> seller_id), CHECK(ends_at <= hard_end)
);
CREATE INDEX economy_auctions_due ON economy_auctions(ends_at) WHERE status='open';
CREATE TABLE economy_trades (
 id BIGSERIAL PRIMARY KEY, user_a BIGINT NOT NULL, user_b BIGINT NOT NULL CHECK(user_b <> user_a),
 offer_a JSONB NOT NULL, offer_b JSONB NOT NULL,
 confirmed_a BOOLEAN NOT NULL DEFAULT false, confirmed_b BOOLEAN NOT NULL DEFAULT false,
 held_b BOOLEAN NOT NULL DEFAULT false,
 expires_at TIMESTAMPTZ NOT NULL,
 status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','completed','cancelled','expired'))
);
CREATE INDEX economy_trades_due ON economy_trades(expires_at) WHERE status='open';
CREATE TABLE economy_settings (
 guild_id BIGINT PRIMARY KEY,
 pool_goal BIGINT NOT NULL DEFAULT 10000 CHECK(pool_goal BETWEEN 100 AND 1000000000),
 pool_total BIGINT NOT NULL DEFAULT 0 CHECK(pool_total >= 0),
 shop_min_level INTEGER NOT NULL DEFAULT 0 CHECK(shop_min_level BETWEEN 0 AND 100),
 shop_min_days INTEGER NOT NULL DEFAULT 0 CHECK(shop_min_days BETWEEN 0 AND 365),
 shop_enabled BOOLEAN NOT NULL DEFAULT true,
 revision BIGINT NOT NULL DEFAULT 0
);
-- First seen locally is a trustworthy minimum-membership timestamp; no backfilled tenure.
ALTER TABLE members ADD COLUMN economy_joined_at TIMESTAMPTZ NOT NULL DEFAULT now();
