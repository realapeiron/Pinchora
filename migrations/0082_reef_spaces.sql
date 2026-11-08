-- Bound lock waits; this migration only changes metadata on existing tables.
SET LOCAL lock_timeout = '2s';
SET LOCAL statement_timeout = '15s';
-- A monotonic house revision covers placements, surfaces and upgrades on every transport.
ALTER TABLE houses ADD COLUMN edit_revision BIGINT NOT NULL DEFAULT 0;
CREATE OR REPLACE FUNCTION reef_layout_changed() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE house_owner BIGINT;
BEGIN
    IF TG_OP = 'DELETE' THEN house_owner := OLD.user_id;
    ELSE house_owner := NEW.user_id; END IF;
    UPDATE houses SET edit_revision=edit_revision+1 WHERE user_id=house_owner;
    IF FOUND THEN
        INSERT INTO reef_layout_outbox(owner_id,transaction_id)
        VALUES(house_owner,txid_current()) ON CONFLICT DO NOTHING;
    END IF;
    RETURN NULL;
END $$;
CREATE TABLE reef_edit_receipts (
    actor_id BIGINT NOT NULL,
    request_id TEXT NOT NULL CHECK(length(request_id) BETWEEN 1 AND 100),
    owner_id BIGINT NOT NULL REFERENCES houses(user_id) ON DELETE CASCADE,
    room_idx SMALLINT NOT NULL,
    payload TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY(actor_id,request_id)
);
CREATE INDEX reef_edit_receipts_retention ON reef_edit_receipts(created_at);
CREATE TABLE reef_space_preferences (
    user_id BIGINT PRIMARY KEY,
    presence BOOLEAN NOT NULL DEFAULT false,
    visits TEXT NOT NULL DEFAULT 'open' CHECK(visits IN ('open','friends','doorbell','closed')),
    doorbell BOOLEAN NOT NULL DEFAULT false
);
CREATE TABLE reef_doorbells (
    id BIGSERIAL PRIMARY KEY,
    host_id BIGINT NOT NULL,
    guest_id BIGINT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','accepted','declined')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL DEFAULT now()+interval '2 minutes',
    CHECK(host_id<>guest_id)
);
CREATE INDEX reef_doorbells_pair ON reef_doorbells(host_id,guest_id,created_at);
CREATE INDEX reef_doorbells_guest_created ON reef_doorbells(guest_id,created_at);
CREATE INDEX reef_doorbells_host_pending ON reef_doorbells(host_id,expires_at) WHERE status='pending';
CREATE INDEX reef_doorbells_retention ON reef_doorbells(expires_at);
-- Shared by HTTP admission and chat transactions; blocks and privacy always win.
CREATE FUNCTION reef_house_admitted(host BIGINT, guest BIGINT) RETURNS BOOLEAN LANGUAGE SQL STABLE AS $$
 SELECT host=guest OR (
 EXISTS(SELECT 1 FROM houses WHERE user_id=host) AND
 NOT EXISTS(SELECT 1 FROM user_privacy WHERE user_id=host AND hide_house) AND
 NOT EXISTS(SELECT 1 FROM guestbook_blocks WHERE (host_id=host AND blocked_id=guest) OR (host_id=guest AND blocked_id=host)) AND
 (NOT EXISTS(SELECT 1 FROM houses WHERE user_id=host AND locked) OR EXISTS(SELECT 1 FROM reef_doorbells WHERE host_id=host AND guest_id=guest AND status='accepted' AND expires_at>now())) AND
 (COALESCE((SELECT visits FROM reef_space_preferences WHERE user_id=host),'open')='open' OR
 (EXISTS(SELECT 1 FROM reef_space_preferences WHERE user_id=host AND visits='friends') AND
 (EXISTS(SELECT 1 FROM friends WHERE user_id=host AND friend_id=guest) OR EXISTS(SELECT 1 FROM partners WHERE user_a=least(host,guest) AND user_b=greatest(host,guest)))) OR
 (EXISTS(SELECT 1 FROM reef_space_preferences WHERE user_id=host AND visits<>'closed') AND EXISTS(SELECT 1 FROM reef_doorbells WHERE host_id=host AND guest_id=guest AND status='accepted' AND expires_at>now())))
 );
$$;
-- Index only a new, empty relation: never scan/lock the live evidence table to build an index.
-- Deleting a note removes the link, while already reported evidence keeps its retention policy.
CREATE TABLE reef_guestbook_evidence (
    note_id BIGINT PRIMARY KEY REFERENCES guestbook_notes(id) ON DELETE CASCADE,
    message_id BIGINT NOT NULL UNIQUE REFERENCES reef_chat_evidence(id) ON DELETE CASCADE
);
