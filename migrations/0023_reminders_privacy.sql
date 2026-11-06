-- Weekly and monthly shell rewards, reminders, privacy choices and the numbers behind Wrapped.

-- When a player last collected a `weekly` or `monthly` reward. The daily one lives on `users`.
CREATE TABLE reward_claims (
    user_id     BIGINT      NOT NULL,
    kind        TEXT        NOT NULL,
    claimed_at  TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (user_id, kind)
);

-- Notifications a player asked for: `daily`, `weekly`, `monthly`, `pet` and `trip` ("tell me by
-- direct message when it is ready").
CREATE TABLE reminder_optins (
    user_id  BIGINT      NOT NULL,
    kind     TEXT        NOT NULL,
    since    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, kind)
);

-- Direct messages waiting for their time. `kind` is `text` (a reminder the player wrote) or one of
-- the opt-in kinds above, scheduled when the thing it is about starts; `ref_id` is the pet of a
-- `pet` or `trip` reminder. A row is claimed with a short lease while its message goes out and is
-- deleted once sent or dropped, so a crash in between only delays the message.
CREATE TABLE reminders (
    id           BIGSERIAL   PRIMARY KEY,
    user_id      BIGINT      NOT NULL,
    kind         TEXT        NOT NULL,
    ref_id       BIGINT      NOT NULL DEFAULT 0,
    body         TEXT        NOT NULL DEFAULT '',
    due_at       TIMESTAMPTZ NOT NULL,
    lease_until  TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX reminders_due_idx ON reminders (due_at);
CREATE INDEX reminders_user_idx ON reminders (user_id, due_at);
-- One scheduled message per thing: a second one replaces the first.
CREATE UNIQUE INDEX reminders_auto_idx ON reminders (user_id, kind, ref_id) WHERE kind <> 'text';

-- What a player hides. No row means nothing is hidden.
CREATE TABLE user_privacy (
    user_id           BIGINT  PRIMARY KEY,
    hide_profile      BOOLEAN NOT NULL DEFAULT FALSE,
    hide_leaderboard  BOOLEAN NOT NULL DEFAULT FALSE,
    hide_house        BOOLEAN NOT NULL DEFAULT FALSE
);

-- The fish a player caught in a calendar year, for the favourite fish of /wrapped.
CREATE TABLE activity_fish (
    user_id  BIGINT  NOT NULL,
    year     INTEGER NOT NULL,
    fish_id  TEXT    NOT NULL,
    caught   INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id, year, fish_id)
);

-- What a player did in a calendar month (UTC, `month` is its first day), for /wrapped.
CREATE TABLE activity_monthly (
    user_id         BIGINT NOT NULL,
    month           DATE   NOT NULL,
    messages        BIGINT NOT NULL DEFAULT 0,
    xp              BIGINT NOT NULL DEFAULT 0,
    shells          BIGINT NOT NULL DEFAULT 0,
    fish            BIGINT NOT NULL DEFAULT 0,
    visits          BIGINT NOT NULL DEFAULT 0,
    likes_given     BIGINT NOT NULL DEFAULT 0,
    likes_received  BIGINT NOT NULL DEFAULT 0,
    pet_care        BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id, month)
);

-- What "delete my data" leaves behind: the id of a player who deleted everything, with the times
-- of what they had used up, so deleting and coming back cannot hand out a fresh daily, weekly or
-- monthly reward, another rep, a second daily wordle reward, more paid visits or likes for the
-- day, or an early pet walk. A returning player starts with nothing else (no shells, no level, no
-- house). The numbers are honoured wherever the player's own rows would have been looked at, and
-- the row is dropped once the longest cooldown (a month) is long gone.
-- `day` is the UTC day `visits` (paid-visit slots used) and `liked` (houses liked) belong to.
CREATE TABLE deleted_users (
    user_id        BIGINT      PRIMARY KEY,
    deleted_at     TIMESTAMPTZ NOT NULL,
    last_daily_at  TIMESTAMPTZ,
    last_rep_at    TIMESTAMPTZ,
    weekly_at      TIMESTAMPTZ,
    monthly_at     TIMESTAMPTZ,
    day            DATE,
    visits         BIGINT      NOT NULL DEFAULT 0,
    liked          BIGINT[]    NOT NULL DEFAULT '{}',
    wordle_day     INTEGER,
    walk_until     TIMESTAMPTZ
);

CREATE INDEX deleted_users_age_idx ON deleted_users (deleted_at);

-- Numbers that stand in for deleted players where a row has to stay for somebody else's sake (a
-- guest's paid visits of the day outlive the host who deleted their data). They are negative, so
-- they never match a real player, and they say nothing about who was deleted.
CREATE SEQUENCE deleted_ids;

-- Everything that hands out something limited per player, or creates the player's row, and the
-- delete of that player's data take this lock first and keep it to the end of their transaction,
-- so the two never overlap: a claim waits for a delete in progress and then sees its tombstone, and
-- a delete waits for a claim in progress and then tombstones the time it set. Locks are taken in id
-- order when a transaction needs two.
CREATE FUNCTION lock_user(player_id BIGINT) RETURNS void AS $$
    SELECT pg_advisory_xact_lock(hashtextextended('user:' || player_id, 0))
$$ LANGUAGE sql;

-- A player's row is created from many places (every first command makes one), so this is where a
-- returning player gets the cooldowns of their last life back: the daily and rep ones on the row
-- itself, the weekly and monthly ones as the claims they were. Everything that reads those reads
-- them as if the player had never deleted anything.
--
-- The lock comes first, and it is taken even when the insert turns out to hit an existing row (a
-- BEFORE trigger runs before that is known). A delete that has removed the row and written the
-- tombstone but not yet committed is invisible here; waiting for it is what makes the tombstone
-- read below the committed one, and what keeps a claim from slipping a fresh row in under it.
CREATE FUNCTION restore_cooldowns() RETURNS trigger AS $$
DECLARE
    tomb deleted_users%ROWTYPE;
BEGIN
    PERFORM lock_user(NEW.id);
    SELECT * INTO tomb FROM deleted_users WHERE user_id = NEW.id;
    IF FOUND THEN
        NEW.last_daily_at := GREATEST(NEW.last_daily_at, tomb.last_daily_at);
        NEW.last_rep_at := GREATEST(NEW.last_rep_at, tomb.last_rep_at);
        IF tomb.weekly_at IS NOT NULL THEN
            INSERT INTO reward_claims (user_id, kind, claimed_at) VALUES (NEW.id, 'weekly', tomb.weekly_at)
            ON CONFLICT (user_id, kind) DO NOTHING;
        END IF;
        IF tomb.monthly_at IS NOT NULL THEN
            INSERT INTO reward_claims (user_id, kind, claimed_at) VALUES (NEW.id, 'monthly', tomb.monthly_at)
            ON CONFLICT (user_id, kind) DO NOTHING;
        END IF;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER users_restore_cooldowns BEFORE INSERT ON users
    FOR EACH ROW EXECUTE FUNCTION restore_cooldowns();
