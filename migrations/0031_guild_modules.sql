-- Which Pinchora modules a server uses, and where. A missing row means enabled everywhere.
-- Modules whose switch already lives in a `guilds` column (leveling, points, welcome, gambling)
-- keep it there; their row only carries the channel rule.
CREATE TABLE guild_modules (
    guild_id      BIGINT   NOT NULL,
    module        TEXT     NOT NULL,
    enabled       BOOLEAN  NOT NULL DEFAULT TRUE,
    channel_mode  TEXT     NOT NULL DEFAULT 'all' CHECK (channel_mode IN ('all', 'only', 'except')),
    channels      BIGINT[] NOT NULL DEFAULT '{}' CHECK (cardinality(channels) <= 50),
    PRIMARY KEY (guild_id, module)
);
