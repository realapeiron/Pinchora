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
