-- Titles, automatic badges, the badge collection and unlock notices.

-- Whether the player chose their profile badges. While false the card shows their best
-- earned badges on its own; saving a choice (even an empty one) sets it. Players who already
-- wear badges have made a choice.
ALTER TABLE users ADD COLUMN badges_set BOOLEAN NOT NULL DEFAULT FALSE;
UPDATE users SET badges_set = TRUE WHERE cardinality(badges) > 0;

-- Badges the player has seen in the collection card, and badges already announced with a
-- "Badge unlocked" notice. Players who exist now keep NULL: the first time either is read it is
-- filled in with what they have already earned, so nobody gets a pile of old badges as news.
-- New players start with empty lists, so what they earn first is new.
ALTER TABLE users ADD COLUMN badges_seen TEXT[];
ALTER TABLE users ADD COLUMN badges_notified TEXT[];
ALTER TABLE users ALTER COLUMN badges_seen SET DEFAULT '{}';
ALTER TABLE users ALTER COLUMN badges_notified SET DEFAULT '{}';

-- The title under the name on the profile and rank cards (content/titles.json); NULL: none.
ALTER TABLE users ADD COLUMN title TEXT;

-- Titles granted outside the stats, such as the house of the week. A row is an unlocked title.
CREATE TABLE IF NOT EXISTS user_titles (user_id BIGINT NOT NULL, title_id TEXT NOT NULL, granted_at TIMESTAMPTZ NOT NULL DEFAULT now(), PRIMARY KEY (user_id, title_id));
