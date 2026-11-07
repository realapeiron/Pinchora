-- F21: server events and RSVP state. Event ids also identify reminder references.
CREATE TABLE event_settings (
    guild_id BIGINT PRIMARY KEY,
    timezone TEXT NOT NULL DEFAULT 'UTC'
);
CREATE TABLE events (
    id BIGSERIAL PRIMARY KEY,
    guild_id BIGINT NOT NULL,
    title TEXT NOT NULL CHECK (char_length(title) BETWEEN 1 AND 100),
    description TEXT NOT NULL DEFAULT '' CHECK (char_length(description) <= 1000),
    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ NOT NULL CHECK (ends_at > starts_at),
    tz TEXT NOT NULL,
    location TEXT NOT NULL DEFAULT '',
    channel_id BIGINT,
    message_id BIGINT,
    host_id BIGINT,
    options JSONB NOT NULL,
    required_role_id BIGINT,
    reminders INT[] NOT NULL DEFAULT ARRAY[60,15],
    discord_event_id BIGINT,
    recurrence JSONB,
    parent_id BIGINT REFERENCES events(id),
    status TEXT NOT NULL DEFAULT 'upcoming' CHECK (status IN ('upcoming','ended','cancelled')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (parent_id, starts_at)
);
CREATE INDEX events_guild_upcoming_idx ON events(guild_id, starts_at) WHERE status = 'upcoming';
CREATE TABLE event_rsvps (
    event_id BIGINT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL,
    option TEXT NOT NULL,
    waitlisted BOOLEAN NOT NULL DEFAULT FALSE,
    responded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY(event_id, user_id)
);
CREATE INDEX event_rsvps_queue_idx ON event_rsvps(event_id, option, responded_at) WHERE waitlisted;
