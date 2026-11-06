-- The text command prefix a server picked (`p!profile`); NULL is the default, `p!`.
ALTER TABLE guilds
    ADD COLUMN prefix TEXT CHECK (char_length(prefix) BETWEEN 1 AND 5 AND prefix !~ '[[:space:]@#`]');
