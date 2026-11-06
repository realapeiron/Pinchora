-- The language a player picked with /language (NULL: follow their Discord client), and a server's
-- default for messages that answer nobody in particular, like welcome cards and level-ups (NULL:
-- English). Codes are `Lang::code` values; one the bot no longer knows reads as NULL.
ALTER TABLE users ADD COLUMN language TEXT;
ALTER TABLE guilds ADD COLUMN language TEXT;
