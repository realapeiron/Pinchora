CREATE TABLE guild_leveling_parity (
    guild_id BIGINT PRIMARY KEY REFERENCES guilds(id) ON DELETE CASCADE,
    config JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE UNIQUE INDEX guild_leveling_vanity ON guild_leveling_parity ((config->>'vanity_slug'))
    WHERE config->>'vanity_slug' IS NOT NULL;

CREATE TABLE member_voice_levels (
    guild_id BIGINT NOT NULL REFERENCES guilds(id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    xp BIGINT NOT NULL DEFAULT 0 CHECK (xp >= 0),
    PRIMARY KEY (guild_id, user_id)
);

CREATE INDEX member_voice_levels_board ON member_voice_levels (guild_id, xp DESC, user_id);

CREATE TABLE voice_level_roles (
    guild_id BIGINT NOT NULL REFERENCES guilds(id) ON DELETE CASCADE,
    role_id BIGINT NOT NULL,
    level INTEGER NOT NULL CHECK (level BETWEEN 1 AND 1000),
    PRIMARY KEY (guild_id, role_id)
);

CREATE TABLE guild_leveling_highlight_posts (
    guild_id BIGINT NOT NULL REFERENCES guilds(id) ON DELETE CASCADE,
    week DATE NOT NULL,
    posted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (guild_id, week)
);
