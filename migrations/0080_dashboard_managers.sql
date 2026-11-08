-- Dashboard grants are guild/role configuration, never player ownership.
CREATE TABLE dashboard_managers (
    guild_id BIGINT NOT NULL CHECK (guild_id > 0),
    role_id BIGINT NOT NULL CHECK (role_id > 0 AND role_id <> guild_id),
    areas TEXT[] NOT NULL CHECK (
        cardinality(areas) BETWEEN 1 AND 7 AND
        array_position(areas, NULL) IS NULL AND
        areas <@ ARRAY['configuration', 'roles', 'safety', 'community', 'feeds', 'stats', 'audit']::TEXT[]
    ),
    PRIMARY KEY (guild_id, role_id)
);
-- Searches inspect at most 500 rows using the existing dashboard_audit_guild_idx.
-- No index build or lock on the live audit table is needed.
