-- Player-run shops: listed items leave the seller's inventory until sold or withdrawn.
CREATE TABLE stall_listings (
    id         BIGSERIAL PRIMARY KEY,
    seller_id  BIGINT      NOT NULL,
    item_id    TEXT        NOT NULL,
    price      BIGINT      NOT NULL CHECK (price > 0),
    listed_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX stall_listings_seller_idx ON stall_listings (seller_id, listed_at);
