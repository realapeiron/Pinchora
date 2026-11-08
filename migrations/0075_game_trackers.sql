CREATE TABLE tracker_settings (
    guild_id BIGINT PRIMARY KEY,
    patch_channels JSONB NOT NULL DEFAULT '{}'::jsonb,
    mc_watch JSONB NOT NULL DEFAULT '[]'::jsonb CHECK (jsonb_typeof(mc_watch) = 'array' AND jsonb_array_length(mc_watch) <= 20),
    mc_news_channel_id BIGINT
);
CREATE TABLE riot_links (
    user_id BIGINT NOT NULL,
    game TEXT NOT NULL CHECK (game IN ('lol', 'valorant')),
    puuid TEXT NOT NULL,
    game_name TEXT NOT NULL,
    tag_line TEXT NOT NULL,
    region TEXT NOT NULL,
    linked_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, game)
);
CREATE TABLE tracker_cache (
    key TEXT PRIMARY KEY,
    user_id BIGINT,
    payload JSONB NOT NULL,
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE patch_seen (
    game TEXT NOT NULL,
    item_id TEXT NOT NULL,
    PRIMARY KEY (game, item_id)
);
