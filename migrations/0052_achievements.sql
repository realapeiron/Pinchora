-- Server configured achievements and the one-time grants they earn.
CREATE TABLE server_achievements (
    id BIGSERIAL PRIMARY KEY,
    guild_id BIGINT NOT NULL REFERENCES guilds(id) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 40),
    description TEXT NOT NULL DEFAULT '' CHECK (char_length(description) <= 100),
    icon TEXT NOT NULL DEFAULT 'star' CHECK (char_length(icon) BETWEEN 1 AND 40),
    kind TEXT NOT NULL CHECK (kind IN ('messages','voice_minutes','level','days_active','invites','stars_received','tickets_closed','reputation','games_won','fish_caught','custom')),
    target BIGINT NOT NULL CHECK (target BETWEEN 1 AND 1000000000),
    reward_role_id BIGINT,
    reward_points BIGINT NOT NULL DEFAULT 0 CHECK (reward_points BETWEEN 0 AND 1000000000),
    announce_channel_id BIGINT,
    secret BOOLEAN NOT NULL DEFAULT FALSE,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (guild_id, name),
    UNIQUE (guild_id, id)
);
CREATE INDEX server_achievements_guild_kind ON server_achievements (guild_id, kind) WHERE enabled;

CREATE TABLE member_achievements (
    guild_id BIGINT NOT NULL,
    user_id BIGINT NOT NULL,
    achievement_id BIGINT NOT NULL,
    earned_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (guild_id, user_id, achievement_id),
    FOREIGN KEY (guild_id, achievement_id) REFERENCES server_achievements (guild_id, id) ON DELETE CASCADE
);
CREATE INDEX member_achievements_user ON member_achievements (user_id);
CREATE INDEX member_achievements_who ON member_achievements (achievement_id, earned_at DESC);
