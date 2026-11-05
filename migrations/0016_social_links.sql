-- Partners: one row per pair, the smaller user id first. A user is in at most one pair; the
-- unique keys cover a user who is always `user_a` or always `user_b`, and the trigger covers
-- one who is `user_a` in one pair and `user_b` in another.
CREATE TABLE partners (
    user_a  BIGINT      NOT NULL,
    user_b  BIGINT      NOT NULL,
    since   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_a),
    UNIQUE (user_b),
    CHECK (user_a < user_b)
);

CREATE FUNCTION partners_one_each() RETURNS trigger AS $$
BEGIN
    -- Pairings that share a user wait for each other. The locks are taken smaller id first, so
    -- two of them can never wait on each other in a circle.
    PERFORM pg_advisory_xact_lock(hashtextextended('partner:' || NEW.user_a, 0));
    PERFORM pg_advisory_xact_lock(hashtextextended('partner:' || NEW.user_b, 0));
    IF EXISTS (
        SELECT 1 FROM partners
        WHERE user_a IN (NEW.user_a, NEW.user_b) OR user_b IN (NEW.user_a, NEW.user_b)
    ) THEN
        RAISE EXCEPTION 'already partnered' USING ERRCODE = 'unique_violation';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER partners_one_each BEFORE INSERT ON partners
    FOR EACH ROW EXECUTE FUNCTION partners_one_each();

-- Friendships, one row for each side, written and removed together.
CREATE TABLE friends (
    user_id    BIGINT      NOT NULL,
    friend_id  BIGINT      NOT NULL,
    since      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, friend_id),
    CHECK (user_id <> friend_id)
);

-- Hugs, pats, waves and high fives: how many `from_user` sent `to_user` of each kind.
CREATE TABLE interactions (
    kind       TEXT        NOT NULL,
    from_user  BIGINT      NOT NULL,
    to_user    BIGINT      NOT NULL,
    times      INTEGER     NOT NULL DEFAULT 0,
    last_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (kind, from_user, to_user)
);

CREATE INDEX interactions_from_idx ON interactions (from_user, kind);
CREATE INDEX interactions_to_idx ON interactions (to_user, kind);

-- The name a player last had when they used a social command, so lists of friends and the
-- partner on a profile card never have to ask Discord.
CREATE TABLE social_names (
    user_id  BIGINT PRIMARY KEY,
    name     TEXT NOT NULL
);
