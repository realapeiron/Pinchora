CREATE TABLE dashboard_notification_reads (
    guild_id BIGINT NOT NULL,
    manager_id BIGINT NOT NULL,
    read_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (guild_id, manager_id)
);
CREATE INDEX tickets_notification_recent ON tickets (guild_id, opened_at DESC);
CREATE INDEX mod_reports_notification_recent ON mod_reports (guild_id, created_at DESC) WHERE status = 'pending';
CREATE INDEX mod_appeals_notification_recent ON mod_appeals (guild_id, created_at DESC) WHERE status = 'pending';
CREATE INDEX modmail_notification_recent ON modmail_threads (guild_id, opened_at DESC);
CREATE INDEX dashboard_audit_notification_recent ON dashboard_audit (guild_id, created_at DESC);
