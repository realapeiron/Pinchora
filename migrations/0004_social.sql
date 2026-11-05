CREATE TABLE house_likes (
    host_id   BIGINT NOT NULL,
    liker_id  BIGINT NOT NULL,
    day       DATE   NOT NULL,
    PRIMARY KEY (host_id, liker_id, day)
);

CREATE TABLE gifts (
    id          BIGSERIAL PRIMARY KEY,
    from_id     BIGINT      NOT NULL,
    to_id       BIGINT      NOT NULL,
    item_id     TEXT        NOT NULL,
    sent_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX gifts_to_idx ON gifts (to_id, sent_at DESC);
