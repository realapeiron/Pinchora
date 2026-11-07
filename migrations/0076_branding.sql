CREATE TABLE guild_branding (
    guild_id BIGINT PRIMARY KEY REFERENCES guilds(id) ON DELETE CASCADE,
    nickname TEXT CHECK (nickname IS NULL OR char_length(nickname) BETWEEN 1 AND 32),
    avatar_upload_id TEXT,
    banner_upload_id TEXT,
    bio TEXT CHECK (bio IS NULL OR char_length(bio) BETWEEN 1 AND 190),
    embed_color INTEGER CHECK (embed_color IS NULL OR embed_color BETWEEN 0 AND 16777215),
    active_embed_color INTEGER CHECK (active_embed_color IS NULL OR active_embed_color BETWEEN 0 AND 16777215),
    applied_at TIMESTAMPTZ,
    last_error TEXT,
    avatar_supported BOOLEAN,
    banner_supported BOOLEAN,
    bio_supported BOOLEAN
);
