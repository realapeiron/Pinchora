-- D4: four event card designs, durable revisions, and reusable message kits.
ALTER TABLE card_designs DROP CONSTRAINT card_designs_kind_check;
ALTER TABLE card_designs ADD CONSTRAINT card_designs_kind_check
    CHECK (kind IN ('welcome', 'goodbye', 'boost', 'level_up'));

CREATE TABLE card_design_history (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    guild_id BIGINT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('welcome', 'goodbye', 'boost', 'level_up')),
    design JSONB,
    delivery TEXT NOT NULL,
    actor_id BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX card_design_history_recent ON card_design_history (guild_id, kind, id DESC);

CREATE TABLE message_kits (
    guild_id BIGINT NOT NULL,
    key TEXT NOT NULL CHECK (key ~ '^[a-z0-9][a-z0-9_-]{0,39}$'),
    name TEXT NOT NULL,
    payload JSONB NOT NULL,
    version BIGINT NOT NULL DEFAULT 1,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (guild_id, key)
);

CREATE TABLE message_kit_history (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    guild_id BIGINT NOT NULL,
    key TEXT NOT NULL,
    name TEXT NOT NULL,
    payload JSONB NOT NULL,
    version BIGINT NOT NULL,
    actor_id BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX message_kit_history_recent ON message_kit_history (guild_id, key, id DESC);

CREATE TABLE card_boost_state (
    guild_id BIGINT NOT NULL,
    user_id BIGINT NOT NULL,
    PRIMARY KEY (guild_id, user_id)
);

CREATE TABLE message_event_kits (
    guild_id BIGINT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('welcome', 'goodbye', 'boost', 'level_up')),
    kit_key TEXT NOT NULL,
    PRIMARY KEY (guild_id, kind),
    FOREIGN KEY (guild_id, kit_key) REFERENCES message_kits(guild_id, key) ON DELETE CASCADE
);

CREATE TABLE card_member_visits (
    guild_id BIGINT NOT NULL,
    user_id BIGINT NOT NULL,
    visits INT NOT NULL DEFAULT 1,
    PRIMARY KEY (guild_id, user_id)
);
