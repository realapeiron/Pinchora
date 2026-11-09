use super::*;

impl Db {
    /// Takes up to `limit` sources that are due at `now` and followed by somebody, for this pod to poll.
    /// They are leased for two minutes: other pods skip them meanwhile, and a pod that dies before it
    /// finishes only delays them.
    ///
    /// `guilds` limits it to sources those servers follow (tests that share a database); `None` is all.
    pub async fn claim_due_sources(
        &self,
        now: DateTime<Utc>,
        limit: i64,
        guilds: Option<&[u64]>,
    ) -> anyhow::Result<Vec<FeedSourceRow>> {
        let guilds: Option<Vec<i64>> = guilds.map(|g| g.iter().copied().map(sql_id).collect());
        Ok(sqlx::query_as::<_, FeedSourceRow>(concat!(
            "WITH due(kind_key, source_key) AS MATERIALIZED (
                 SELECT kind, source FROM feed_sources s
                 WHERE subscribers > 0 AND next_poll_at <= $1 AND (lease_until IS NULL OR lease_until < now())
                   AND ($4::bigint[] IS NULL OR EXISTS (
                       SELECT 1 FROM feeds f WHERE f.kind = s.kind AND f.source = s.source AND f.guild_id = ANY($4)))
                 ORDER BY next_poll_at LIMIT $2
                 FOR UPDATE SKIP LOCKED
             )
             UPDATE feed_sources SET lease_until = now() + make_interval(secs => $3)
             FROM due WHERE kind = due.kind_key AND source = due.source_key
             RETURNING ",
            source_columns!()
        ))
        .bind(now)
        .bind(limit)
        .bind(LEASE_SECS)
        .bind(guilds)
        .fetch_all(self.pool())
        .await?)
    }

    /// Records a fetch that failed: `fail_count` goes up, `error` is shown on the dashboard, and the
    /// source is due again at `next_poll_at`.
    pub async fn record_feed_failure(
        &self,
        kind: &str,
        source: &str,
        error: &str,
        next_poll_at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE feed_sources SET fail_count = fail_count + 1, last_error = $3, next_poll_at = $4,
                 last_checked_at = now(), lease_until = NULL, updated_at = now()
             WHERE kind = $1 AND source = $2",
        )
        .bind(kind)
        .bind(source)
        .bind(error)
        .bind(next_poll_at)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Records a fetch the source answered with "not modified": healthy, nothing new. A 304
    /// does not refresh seen history, since the older ids may have left the latest response.
    pub async fn record_feed_unchanged(
        &self,
        kind: &str,
        source: &str,
        next_poll_at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE feed_sources SET fail_count = 0, last_error = NULL, next_poll_at = $3, last_checked_at = now(),
                 lease_until = NULL, updated_at = now()
             WHERE kind = $1 AND source = $2",
        )
        .bind(kind)
        .bind(source)
        .bind(next_poll_at)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Records a successful fetch and queues the posts for what is new: an item is new only for the
    /// transaction that first records it as seen, so two pods never queue it twice. The first fetch of a
    /// source only records what is there. Returns how many posts were queued.
    pub async fn record_feed_poll(&self, polled: &Polled<'_>) -> anyhow::Result<usize> {
        let (kind, source) = (polled.kind, polled.source);
        let mut tx = self.pool().begin().await?;
        let primed: Option<(bool,)> =
            sqlx::query_as("SELECT primed FROM feed_sources WHERE kind = $1 AND source = $2 FOR UPDATE")
                .bind(kind)
                .bind(source)
                .fetch_optional(&mut *tx)
                .await?;
        let Some((primed,)) = primed else {
            return Ok(0);
        };
        let newest = polled.items.iter().filter_map(|i| i.published).max();
        sqlx::query(
            "UPDATE feed_sources SET title = COALESCE($3, title), etag = $4, last_modified = $5, interval_secs = $6,
                 next_poll_at = $7, fail_count = 0, last_error = NULL, last_checked_at = now(),
                 last_item_at = COALESCE($8, last_item_at), primed = TRUE, lease_until = NULL, updated_at = now()
             WHERE kind = $1 AND source = $2",
        )
        .bind(kind)
        .bind(source)
        .bind(polled.title)
        .bind(polled.etag)
        .bind(polled.last_modified)
        .bind(polled.interval_secs)
        .bind(polled.next_poll_at)
        .bind(newest)
        .execute(&mut *tx)
        .await?;
        if let Some(title) = polled.title {
            sqlx::query(
                "UPDATE feeds SET display_name = $3
                 WHERE kind = $1 AND source = $2 AND display_name IS DISTINCT FROM $3",
            )
            .bind(kind)
            .bind(source)
            .bind(title)
            .execute(&mut *tx)
            .await?;
        }
        let ids: Vec<String> = polled.items.iter().map(|i| i.id.clone()).collect();
        let fresh = mark_seen(&mut tx, kind, source, &ids).await?;
        let mut queued = 0;
        if primed && !fresh.is_empty() {
            let new: Vec<Item> = polled.items.iter().filter(|i| fresh.contains(&i.id)).cloned().collect();
            let announce = to_announce(new, polled.now);
            if !announce.is_empty() {
                let feeds = sqlx::query_as::<_, FeedRow>(concat!(
                    "SELECT ",
                    feed_columns!(),
                    " FROM feeds WHERE kind = $1 AND source = $2 AND enabled ORDER BY id"
                ))
                .bind(kind)
                .bind(source)
                .fetch_all(&mut *tx)
                .await?;
                for item in &announce {
                    let payload = serde_json::to_string(item)?;
                    for feed in &feeds {
                        if !feed_wants(item, feed.created_at, &feed.filter_include, &feed.filter_exclude) {
                            continue;
                        }
                        queued += sqlx::query(
                            "INSERT INTO feed_outbox (feed_id, item_id, item, created_at)
                             VALUES ($1, $2, $3::jsonb, clock_timestamp())
                             ON CONFLICT DO NOTHING",
                        )
                        .bind(feed.id)
                        .bind(&item.id)
                        .bind(&payload)
                        .execute(&mut *tx)
                        .await?
                        .rows_affected() as usize;
                    }
                }
            }
        }
        tx.commit().await?;
        Ok(queued)
    }

    /// Takes up to `limit` waiting posts, oldest first, leased for two minutes.
    /// `guilds` limits it as for [`Db::claim_due_sources`].
    pub async fn claim_feed_posts(&self, limit: i64, guilds: Option<&[u64]>) -> anyhow::Result<Vec<FeedPost>> {
        let guilds: Option<Vec<i64>> = guilds.map(|g| g.iter().copied().map(sql_id).collect());
        let rows: Vec<(i64, String, String, i32)> = sqlx::query_as(
            "WITH due(feed_key, item_key) AS MATERIALIZED (
                 SELECT feed_id, item_id FROM feed_outbox
                 WHERE (lease_until IS NULL OR lease_until < now())
                   AND ($3::bigint[] IS NULL OR feed_id IN (SELECT id FROM feeds WHERE guild_id = ANY($3)))
                 ORDER BY created_at, feed_id, item_id LIMIT $1
                 FOR UPDATE SKIP LOCKED
             )
             UPDATE feed_outbox SET lease_until = now() + make_interval(secs => $2), attempts = attempts + 1
             FROM due WHERE feed_id = due.feed_key AND item_id = due.item_key
             RETURNING feed_id, item_id, item::text, attempts",
        )
        .bind(limit)
        .bind(LEASE_SECS)
        .bind(guilds)
        .fetch_all(self.pool())
        .await?;
        let mut posts = Vec::with_capacity(rows.len());
        for (feed_id, item_id, item, attempts) in rows {
            let feed = sqlx::query_as::<_, FeedRow>(concat!("SELECT ", feed_columns!(), " FROM feeds WHERE id = $1"))
                .bind(feed_id)
                .fetch_optional(self.pool())
                .await?;
            let item = serde_json::from_str::<Item>(&item);
            match (feed, item) {
                (Some(feed), Ok(item)) => posts.push(FeedPost { feed, item, attempts }),
                // Unreadable (written by a newer version?) or its feed is gone: nothing to send.
                _ => self.drop_feed_post(feed_id, &item_id).await?,
            }
        }
        posts.sort_by_key(|p| (p.item.published, p.feed.id));
        Ok(posts)
    }

    async fn drop_feed_post(&self, feed: i64, item: &str) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM feed_outbox WHERE feed_id = $1 AND item_id = $2")
            .bind(feed)
            .bind(item)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Extends only this claim's lease. A reclaimed post has a newer attempt number.
    pub async fn renew_feed_post(&self, post: &FeedPost) -> anyhow::Result<bool> {
        Ok(sqlx::query(
            "UPDATE feed_outbox SET lease_until = now() + make_interval(secs => $4)
             WHERE feed_id = $1 AND item_id = $2 AND attempts = $3 AND lease_until > now()",
        )
        .bind(post.feed.id)
        .bind(&post.item.id)
        .bind(post.attempts)
        .bind(LEASE_SECS)
        .execute(self.pool())
        .await?
        .rows_affected()
            == 1)
    }

    /// Records how a claimed post went: sent or given up, it leaves the outbox; a retry waits for its lease.
    /// A subscription Discord keeps refusing is switched off after [`FEED_REFUSALS_OFF`] refusals in a row.
    pub async fn finish_feed_post(&self, post: &FeedPost, outcome: &PostOutcome) -> anyhow::Result<()> {
        let (feed, item) = (post.feed.id, post.item.id.as_str());
        let mut tx = self.pool().begin().await?;
        if matches!(outcome, PostOutcome::Retry(_)) && post.attempts < FEED_POST_ATTEMPTS {
            return Ok(());
        }
        let deleted = sqlx::query("DELETE FROM feed_outbox WHERE feed_id = $1 AND item_id = $2 AND attempts = $3")
            .bind(feed)
            .bind(item)
            .bind(post.attempts)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if deleted == 0 {
            return Ok(());
        }
        match outcome {
            PostOutcome::Sent => {
                sqlx::query(
                    "UPDATE feeds SET last_sent_at = now(), last_item_id = $2, last_error = NULL, delivery_failures = 0
                     WHERE id = $1",
                )
                .bind(feed)
                .bind(item)
                .execute(&mut *tx)
                .await?;
            }
            PostOutcome::Refused(error) | PostOutcome::Retry(error) => {
                let switched: Option<(bool, String, String)> = sqlx::query_as(
                    "UPDATE feeds SET last_error = $2, delivery_failures = delivery_failures + 1,
                         enabled = enabled AND delivery_failures + 1 < $3
                     WHERE id = $1 RETURNING enabled, kind, source",
                )
                .bind(feed)
                .bind(error)
                .bind(FEED_REFUSALS_OFF)
                .fetch_optional(&mut *tx)
                .await?;
                if let Some((false, kind, source)) = switched {
                    count_subscribers(&mut tx, &kind, &source).await?;
                }
            }
            PostOutcome::Dropped => {}
        }
        tx.commit().await?;
        Ok(())
    }

    /// Forgets what a feed no longer shows (items not seen in a fetch since `before`), posts that waited
    /// a day, and sources nobody has followed for a day. Returns the rows removed.
    pub async fn forget_old_feed_items(&self, before: DateTime<Utc>) -> anyhow::Result<u64> {
        let seen = sqlx::query("DELETE FROM feed_items_seen WHERE seen_at < $1")
            .bind(before)
            .execute(self.pool())
            .await?
            .rows_affected();
        let posts = sqlx::query("DELETE FROM feed_outbox WHERE created_at < now() - interval '1 day'")
            .execute(self.pool())
            .await?
            .rows_affected();
        let webhooks = sqlx::query("DELETE FROM feed_webhook_messages WHERE received_at < now() - interval '7 days'")
            .execute(self.pool())
            .await?
            .rows_affected();
        let sources = sqlx::query(
            "WITH gone AS (
                 DELETE FROM feed_sources s
                 WHERE subscribers = 0 AND updated_at < now() - interval '1 day'
                   AND NOT EXISTS (SELECT 1 FROM feeds f WHERE f.kind = s.kind AND f.source = s.source)
                 RETURNING kind, source
             )
             DELETE FROM feed_items_seen i USING gone WHERE i.kind = gone.kind AND i.source = gone.source",
        )
        .execute(self.pool())
        .await?
        .rows_affected();
        Ok(seen + posts + webhooks + sources)
    }
}
