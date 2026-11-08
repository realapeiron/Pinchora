//! Social feeds: the sources the poller fetches (`feed_sources`), the servers' subscriptions (`feeds`),
//! what was already announced (`feed_items_seen`) and the posts still to send (`feed_outbox`).
//!
//! Every step is safe on many pods at once: a source is leased while one pod polls it, an item is new
//! for exactly the transaction that first records it as seen, and a post is leased while it goes out.

use chrono::{DateTime, Utc};
use pinchora_core::feeds::{Item, MAX_FEEDS, feed_wants, to_announce};

use crate::{Db, sql_id};

/// How long a claimed source or post stays out of other pods' reach.
const LEASE_SECS: i64 = 120;
/// A post that failed this many times for a reason worth retrying is given up.
pub const FEED_POST_ATTEMPTS: i32 = 5;
/// After this many posts in a row that Discord refused (no permission, channel gone), a feed is switched off.
pub const FEED_REFUSALS_OFF: i32 = 10;

/// The columns of [`FeedRow`], for `concat!` (queries must be literal SQL).
macro_rules! feed_columns {
    () => {
        "id, guild_id, kind, source, display_name, channel_id, message, embed, ping_role_id, filter_include, \
         filter_exclude, enabled, last_item_id, last_sent_at, last_error, delivery_failures, created_by, created_at, live_role_id, live_message_id"
    };
}

/// A server's subscription to a source.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct FeedRow {
    pub id: i64,
    pub guild_id: i64,
    pub kind: String,
    pub source: String,
    pub display_name: Option<String>,
    pub channel_id: i64,
    pub message: Option<String>,
    pub embed: bool,
    pub ping_role_id: Option<i64>,
    pub filter_include: Vec<String>,
    pub filter_exclude: Vec<String>,
    pub enabled: bool,
    pub last_item_id: Option<String>,
    pub last_sent_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub delivery_failures: i32,
    pub created_by: i64,
    pub created_at: DateTime<Utc>,
    pub live_role_id: Option<i64>,
    pub live_message_id: Option<i64>,
}

/// What the poller knows about a source.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct FeedSourceRow {
    pub kind: String,
    pub source: String,
    pub title: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub next_poll_at: DateTime<Utc>,
    pub interval_secs: i32,
    pub fail_count: i32,
    pub last_error: Option<String>,
    pub last_checked_at: Option<DateTime<Utc>>,
    pub last_item_at: Option<DateTime<Utc>>,
    pub primed: bool,
    pub subscribers: i32,
    pub live_stream_id: Option<String>,
    pub broadcaster_id: Option<String>,
    pub eventsub_registered_at: Option<DateTime<Utc>>,
}

/// The columns of [`FeedSourceRow`].
macro_rules! source_columns {
    () => {
        "kind, source, title, etag, last_modified, next_poll_at, interval_secs, fail_count, last_error, \
         last_checked_at, last_item_at, primed, subscribers, live_stream_id, broadcaster_id, eventsub_registered_at"
    };
}

/// A subscription with its source, for the lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedListRow {
    pub feed: FeedRow,
    pub source: FeedSourceRow,
}

/// A new subscription.
#[derive(Debug, Clone)]
pub struct NewFeed<'a> {
    pub guild: u64,
    pub kind: &'a str,
    pub source: &'a str,
    /// The source's name, as the check that found it read it.
    pub title: Option<&'a str>,
    pub channel: u64,
    pub message: Option<&'a str>,
    pub embed: bool,
    pub ping_role: Option<u64>,
    pub live_role: Option<u64>,
    pub include: &'a [String],
    pub exclude: &'a [String],
    pub created_by: u64,
    /// The source's items when it was checked, recorded as seen so following it does not post them.
    pub seen: &'a [String],
    pub interval_secs: i32,
}

/// How adding a subscription went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedAdd {
    Added(Box<FeedRow>),
    /// The server already follows [`MAX_FEEDS`] sources.
    Full,
    /// The server already posts this source in that channel.
    Duplicate,
}

/// A change to a subscription; `None` leaves a field as it is.
#[derive(Debug, Clone, Default)]
pub struct FeedEdit {
    pub channel: Option<u64>,
    /// `Some(None)` goes back to the default message.
    pub message: Option<Option<String>>,
    pub embed: Option<bool>,
    pub ping_role: Option<Option<u64>>,
    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
    pub enabled: Option<bool>,
    pub live_role: Option<Option<u64>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FeedChannelLink {
    pub kind: String,
    pub source: String,
    pub verification_code: Option<String>,
    pub code_expires_at: Option<DateTime<Utc>>,
    pub verified_at: Option<DateTime<Utc>>,
}

/// How changing a subscription went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedChange {
    Changed {
        before: Box<FeedRow>,
        after: Box<FeedRow>,
    },
    NotFound,
    /// The server already posts this source in the channel it was moved to.
    Duplicate,
}

/// What one successful fetch of a source found.
#[derive(Debug, Clone)]
pub struct Polled<'a> {
    pub kind: &'a str,
    pub source: &'a str,
    pub title: Option<&'a str>,
    pub etag: Option<&'a str>,
    pub last_modified: Option<&'a str>,
    pub interval_secs: i32,
    pub next_poll_at: DateTime<Utc>,
    /// Newest first, as the source lists them.
    pub items: &'a [Item],
    pub now: DateTime<Utc>,
}

/// A post waiting to go out, with the subscription it is for.
#[derive(Debug, Clone)]
pub struct FeedPost {
    pub feed: FeedRow,
    pub item: Item,
    /// Incremented on each claim; stale workers cannot complete a newer claim.
    pub attempts: i32,
}

/// How sending a post went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostOutcome {
    Sent,
    /// Discord refused it for good (no permission, the channel is gone): the reason, for the dashboard.
    Refused(String),
    /// It may work later (Discord was down): tried again after the lease, given up after
    /// [`FEED_POST_ATTEMPTS`].
    Retry(String),
    /// The subscription is off or its module is switched off: dropped quietly.
    Dropped,
}

impl Db {
    /// A server's subscriptions with their sources, oldest first.
    pub async fn feeds_of(&self, guild: u64) -> anyhow::Result<Vec<FeedListRow>> {
        let feeds = sqlx::query_as::<_, FeedRow>(concat!(
            "SELECT ",
            feed_columns!(),
            " FROM feeds WHERE guild_id = $1 ORDER BY id"
        ))
        .bind(sql_id(guild))
        .fetch_all(self.pool())
        .await?;
        let mut rows = Vec::with_capacity(feeds.len());
        for feed in feeds {
            let source = self.feed_source(&feed.kind, &feed.source).await?;
            if let Some(source) = source {
                rows.push(FeedListRow { feed, source });
            }
        }
        Ok(rows)
    }

    /// One of a server's subscriptions.
    pub async fn feed(&self, guild: u64, id: i64) -> anyhow::Result<Option<FeedRow>> {
        Ok(sqlx::query_as::<_, FeedRow>(concat!(
            "SELECT ",
            feed_columns!(),
            " FROM feeds WHERE guild_id = $1 AND id = $2"
        ))
        .bind(sql_id(guild))
        .bind(id)
        .fetch_optional(self.pool())
        .await?)
    }

    pub async fn feed_source(&self, kind: &str, source: &str) -> anyhow::Result<Option<FeedSourceRow>> {
        Ok(sqlx::query_as::<_, FeedSourceRow>(concat!(
            "SELECT ",
            source_columns!(),
            " FROM feed_sources WHERE kind = $1 AND source = $2"
        ))
        .bind(kind)
        .bind(source)
        .fetch_optional(self.pool())
        .await?)
    }

    /// Adds a subscription, at most [`MAX_FEEDS`] per server: the source is created (or woken up) and the
    /// items it showed when checked are recorded as seen only if no other subscriber exists.
    pub async fn add_feed(&self, new: &NewFeed<'_>) -> anyhow::Result<FeedAdd> {
        let guild = sql_id(new.guild);
        let mut tx = self.pool().begin().await?;
        // Two adds at once count each other.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('feeds:' || $1::text, 0))")
            .bind(guild)
            .execute(&mut *tx)
            .await?;
        let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM feeds WHERE guild_id = $1")
            .bind(guild)
            .fetch_one(&mut *tx)
            .await?;
        if count >= MAX_FEEDS {
            return Ok(FeedAdd::Full);
        }
        sqlx::query(
            "INSERT INTO feed_sources (kind, source, title, interval_secs) VALUES ($1, $2, $3, $4)
             ON CONFLICT (kind, source) DO UPDATE SET
                 title = COALESCE(EXCLUDED.title, feed_sources.title),
                 next_poll_at = CASE WHEN feed_sources.subscribers = 0 THEN now() ELSE feed_sources.next_poll_at END,
                 updated_at = now()",
        )
        .bind(new.kind)
        .bind(new.source)
        .bind(new.title)
        .bind(new.interval_secs)
        .execute(&mut *tx)
        .await?;
        let (subscribers,): (i32,) =
            sqlx::query_as("SELECT subscribers FROM feed_sources WHERE kind = $1 AND source = $2 FOR UPDATE")
                .bind(new.kind)
                .bind(new.source)
                .fetch_one(&mut *tx)
                .await?;
        let added = sqlx::query_as::<_, FeedRow>(concat!(
            "INSERT INTO feeds (guild_id, kind, source, display_name, channel_id, message, embed, ping_role_id,
                                filter_include, filter_exclude, created_by, live_role_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
             ON CONFLICT (guild_id, kind, source, channel_id) DO NOTHING
             RETURNING ",
            feed_columns!()
        ))
        .bind(guild)
        .bind(new.kind)
        .bind(new.source)
        .bind(new.title)
        .bind(sql_id(new.channel))
        .bind(new.message)
        .bind(new.embed)
        .bind(new.ping_role.map(sql_id))
        .bind(new.include)
        .bind(new.exclude)
        .bind(sql_id(new.created_by))
        .bind(new.live_role.map(sql_id))
        .fetch_optional(&mut *tx)
        .await?;
        let Some(added) = added else {
            return Ok(FeedAdd::Duplicate);
        };
        if subscribers == 0 {
            mark_seen(&mut tx, new.kind, new.source, new.seen).await?;
            sqlx::query("UPDATE feed_sources SET primed = TRUE WHERE kind = $1 AND source = $2")
                .bind(new.kind)
                .bind(new.source)
                .execute(&mut *tx)
                .await?;
        }
        count_subscribers(&mut tx, new.kind, new.source).await?;
        tx.commit().await?;
        Ok(FeedAdd::Added(Box::new(added)))
    }

    /// Changes a subscription of `guild`.
    pub async fn edit_feed(&self, guild: u64, id: i64, edit: &FeedEdit) -> anyhow::Result<FeedChange> {
        let mut tx = self.pool().begin().await?;
        let before = sqlx::query_as::<_, FeedRow>(concat!(
            "SELECT ",
            feed_columns!(),
            " FROM feeds WHERE guild_id = $1 AND id = $2 FOR UPDATE"
        ))
        .bind(sql_id(guild))
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(before) = before else {
            return Ok(FeedChange::NotFound);
        };
        let channel = edit.channel.map_or(before.channel_id, sql_id);
        let message = edit.message.clone().unwrap_or_else(|| before.message.clone());
        let ping_role = edit.ping_role.map_or(before.ping_role_id, |r| r.map(sql_id));
        let enabled = edit.enabled.unwrap_or(before.enabled);
        let after = sqlx::query_as::<_, FeedRow>(concat!(
            "UPDATE feeds SET channel_id = $3, message = $4, embed = $5, ping_role_id = $6, filter_include = $7,
                 filter_exclude = $8, enabled = $9, live_role_id = $10,
                 last_error = CASE WHEN $9 AND NOT enabled THEN NULL ELSE last_error END,
                 delivery_failures = CASE WHEN ($9 AND NOT enabled) OR channel_id <> $3 THEN 0
                                          ELSE delivery_failures END
             WHERE guild_id = $1 AND id = $2
               AND NOT EXISTS (SELECT 1 FROM feeds o WHERE o.guild_id = $1 AND o.kind = feeds.kind
                               AND o.source = feeds.source AND o.channel_id = $3 AND o.id <> $2)
             RETURNING ",
            feed_columns!()
        ))
        .bind(sql_id(guild))
        .bind(id)
        .bind(channel)
        .bind(&message)
        .bind(edit.embed.unwrap_or(before.embed))
        .bind(ping_role)
        .bind(edit.include.as_ref().unwrap_or(&before.filter_include))
        .bind(edit.exclude.as_ref().unwrap_or(&before.filter_exclude))
        .bind(enabled)
        .bind(edit.live_role.map_or(before.live_role_id, |r| r.map(sql_id)))
        .fetch_optional(&mut *tx)
        .await?;
        let Some(after) = after else {
            return Ok(FeedChange::Duplicate);
        };
        if after.enabled != before.enabled {
            count_subscribers(&mut tx, &after.kind, &after.source).await?;
        }
        tx.commit().await?;
        Ok(FeedChange::Changed {
            before: Box::new(before),
            after: Box::new(after),
        })
    }

    /// Removes a subscription of `guild`, and what it still had to post.
    pub async fn remove_feed(&self, guild: u64, id: i64) -> anyhow::Result<Option<FeedRow>> {
        let mut tx = self.pool().begin().await?;
        let removed = sqlx::query_as::<_, FeedRow>(concat!(
            "DELETE FROM feeds WHERE guild_id = $1 AND id = $2 RETURNING ",
            feed_columns!()
        ))
        .bind(sql_id(guild))
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(feed) = &removed {
            count_subscribers(&mut tx, &feed.kind, &feed.source).await?;
        }
        tx.commit().await?;
        Ok(removed)
    }

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

    /// A live source's current stream. Returns announcements to edit when it goes offline.
    pub async fn set_live_stream(
        &self,
        kind: &str,
        source: &str,
        broadcaster_id: &str,
        stream_id: Option<&str>,
    ) -> anyhow::Result<Vec<FeedRow>> {
        let mut tx = self.pool().begin().await?;
        let previous: Option<(Option<String>,)> =
            sqlx::query_as("SELECT live_stream_id FROM feed_sources WHERE kind = $1 AND source = $2 FOR UPDATE")
                .bind(kind)
                .bind(source)
                .fetch_optional(&mut *tx)
                .await?;
        let Some((previous,)) = previous else {
            return Ok(Vec::new());
        };
        sqlx::query("UPDATE feed_sources SET live_stream_id = $3, broadcaster_id = $4 WHERE kind = $1 AND source = $2")
            .bind(kind)
            .bind(source)
            .bind(stream_id)
            .bind(broadcaster_id)
            .execute(&mut *tx)
            .await?;
        if previous.as_deref() != stream_id {
            sqlx::query(
                "DELETE FROM feed_outbox o USING feeds f
                WHERE o.feed_id = f.id AND f.kind = $1 AND f.source = $2
                AND ($3::text IS NULL OR o.item_id <> $3)",
            )
            .bind(kind)
            .bind(source)
            .bind(stream_id)
            .execute(&mut *tx)
            .await?;
        }
        let ended = if (previous.is_some() && previous.as_deref() != stream_id) || stream_id.is_none() {
            sqlx::query_as::<_, FeedRow>(concat!(
                "SELECT ",
                feed_columns!(),
                " FROM feeds WHERE kind = $1 AND source = $2 AND live_message_id IS NOT NULL"
            ))
            .bind(kind)
            .bind(source)
            .fetch_all(&mut *tx)
            .await?
        } else {
            Vec::new()
        };
        tx.commit().await?;
        Ok(ended)
    }

    pub async fn set_live_message(&self, feed: i64, message: u64, stream: &str) -> anyhow::Result<bool> {
        let written = sqlx::query(
            "UPDATE feeds f SET live_message_id = $2 WHERE id = $1
            AND EXISTS (SELECT 1 FROM feed_sources s WHERE s.kind = f.kind AND s.source = f.source
                AND s.live_stream_id = $3)",
        )
        .bind(feed)
        .bind(sql_id(message))
        .bind(stream)
        .execute(self.pool())
        .await?;
        Ok(written.rows_affected() == 1)
    }

    pub async fn clear_live_message(&self, feed: i64, message: u64) -> anyhow::Result<()> {
        sqlx::query("UPDATE feeds SET live_message_id = NULL WHERE id = $1 AND live_message_id = $2")
            .bind(feed)
            .bind(sql_id(message))
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// A verified member who owns this public channel, if one exists.
    pub async fn linked_streamer(&self, kind: &str, source: &str) -> anyhow::Result<Option<u64>> {
        let row: Option<(i64,)> = sqlx::query_as(
            "SELECT user_id FROM feed_channel_links WHERE kind = $1 AND source = $2 AND verified_at IS NOT NULL",
        )
        .bind(kind)
        .bind(source)
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|(id,)| id as u64))
    }

    /// Serializes a member's channel mutations with role grants on every pod.
    pub async fn lock_feed_link(&self, user: u64) -> anyhow::Result<sqlx::Transaction<'_, sqlx::Postgres>> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("SET LOCAL lock_timeout = '2s'").execute(&mut *tx).await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 7192))")
            .bind(user.to_string())
            .execute(&mut *tx)
            .await?;
        Ok(tx)
    }

    pub async fn channel_links(&self, user: u64) -> anyhow::Result<Vec<FeedChannelLink>> {
        Ok(sqlx::query_as(
            "SELECT kind, source, verification_code, code_expires_at, verified_at
            FROM feed_channel_links WHERE user_id = $1 ORDER BY kind",
        )
        .bind(sql_id(user))
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn feed_channel_is_live(&self, kind: &str, source: &str) -> anyhow::Result<bool> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM feed_sources WHERE kind = $1 AND source = $2
            AND live_stream_id IS NOT NULL)",
        )
        .bind(kind)
        .bind(source)
        .fetch_one(self.pool())
        .await?)
    }

    pub async fn live_feeds_for_channel(&self, kind: &str, source: &str) -> anyhow::Result<Vec<FeedRow>> {
        Ok(sqlx::query_as::<_, FeedRow>(concat!(
            "SELECT ",
            feed_columns!(),
            " FROM feeds f WHERE f.kind = $1 AND f.source = $2
              AND f.live_role_id IS NOT NULL AND f.live_message_id IS NOT NULL"
        ))
        .bind(kind)
        .bind(source)
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn start_channel_link(&self, user: u64, kind: &str, source: &str, code: &str) -> anyhow::Result<bool> {
        let mut tx = self.pool().begin().await?;
        // Serialize contenders for a channel and release expired reservations.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 7191))")
            .bind(format!("{kind}:{source}"))
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "DELETE FROM feed_channel_links WHERE kind = $1 AND source = $2
            AND verified_at IS NULL AND code_expires_at <= now()",
        )
        .bind(kind)
        .bind(source)
        .execute(&mut *tx)
        .await?;
        let saved = sqlx::query(
            "INSERT INTO feed_channel_links (user_id, kind, source, verification_code, code_expires_at)
            VALUES ($1, $2, $3, $4, now() + interval '15 minutes')
            ON CONFLICT (user_id, kind) DO UPDATE SET source = EXCLUDED.source,
                verification_code = EXCLUDED.verification_code, code_expires_at = EXCLUDED.code_expires_at,
                verified_at = NULL",
        )
        .bind(sql_id(user))
        .bind(kind)
        .bind(source)
        .bind(code)
        .execute(&mut *tx)
        .await;
        match saved {
            Ok(_) => {
                tx.commit().await?;
                Ok(true)
            }
            Err(sqlx::Error::Database(error)) if error.code().is_some_and(|code| code == "23505") => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    pub async fn verify_channel_link(&self, user: u64, kind: &str, source: &str, code: &str) -> anyhow::Result<bool> {
        Ok(sqlx::query(
            "UPDATE feed_channel_links SET verified_at = now(), verification_code = NULL,
            code_expires_at = NULL WHERE user_id = $1 AND kind = $2 AND source = $3 AND
            verification_code = $4 AND code_expires_at > now()",
        )
        .bind(sql_id(user))
        .bind(kind)
        .bind(source)
        .bind(code)
        .execute(self.pool())
        .await?
        .rows_affected()
            == 1)
    }

    pub async fn unlink_channel(&self, user: u64, kind: &str) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM feed_channel_links WHERE user_id = $1 AND kind = $2")
            .bind(sql_id(user))
            .bind(kind)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// A verified webhook wakes the source once across pods; retries are ignored for a week.
    pub async fn record_twitch_webhook(&self, id: &str, source: &str) -> anyhow::Result<bool> {
        let mut tx = self.pool().begin().await?;
        let fresh = sqlx::query("INSERT INTO feed_webhook_messages (message_id) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(id)
            .execute(&mut *tx)
            .await?
            .rows_affected()
            == 1;
        if fresh {
            sqlx::query("UPDATE feed_sources SET next_poll_at = LEAST(next_poll_at, now()) WHERE kind = 'twitch' AND source = $1")
                .bind(source).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(fresh)
    }

    /// Challenge retries return the same authenticated response, with one durable replay record.
    pub async fn record_twitch_challenge(&self, id: &str) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO feed_webhook_messages (message_id) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn twitch_subscription_source(&self, broadcaster: &str) -> anyhow::Result<Option<String>> {
        Ok(sqlx::query_scalar(
            "SELECT source FROM feed_sources WHERE kind = 'twitch'
            AND broadcaster_id = $1 AND subscribers > 0",
        )
        .bind(broadcaster)
        .fetch_optional(self.pool())
        .await?)
    }

    pub async fn record_twitch_revocation(&self, id: &str, broadcaster: &str) -> anyhow::Result<()> {
        let mut tx = self.pool().begin().await?;
        let fresh = sqlx::query("INSERT INTO feed_webhook_messages (message_id) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(id)
            .execute(&mut *tx)
            .await?
            .rows_affected()
            == 1;
        if fresh {
            sqlx::query(
                "UPDATE feed_sources SET eventsub_registered_at = NULL, next_poll_at = now()
                WHERE kind = 'twitch' AND broadcaster_id = $1",
            )
            .bind(broadcaster)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn mark_eventsub_registered(&self, source: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE feed_sources SET eventsub_registered_at = now() WHERE kind = 'twitch' AND source = $1")
            .bind(source)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn revoke_eventsub(&self, broadcaster_id: &str) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE feed_sources SET eventsub_registered_at = NULL, next_poll_at = now()
            WHERE kind = 'twitch' AND broadcaster_id = $1",
        )
        .bind(broadcaster_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }
}

/// Records `ids` as seen now; returns the ones that were not seen before.
async fn mark_seen(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    kind: &str,
    source: &str,
    ids: &[String],
) -> anyhow::Result<Vec<String>> {
    // One statement may touch a row once: a feed listing an item twice must not trip it.
    let mut ids = ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows: Vec<(String, bool)> = sqlx::query_as(
        "INSERT INTO feed_items_seen (kind, source, item_id)
         SELECT $1, $2, unnest($3::text[])
         ON CONFLICT (kind, source, item_id) DO UPDATE SET seen_at = now()
         RETURNING item_id, (xmax = 0)",
    )
    .bind(kind)
    .bind(source)
    .bind(&ids)
    .fetch_all(&mut **tx)
    .await?;
    Ok(rows.into_iter().filter(|(_, new)| *new).map(|(id, _)| id).collect())
}

/// Sets a source's count of enabled subscriptions.
async fn count_subscribers(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    kind: &str,
    source: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE feed_sources SET updated_at = now(),
             subscribers = (SELECT count(*) FROM feeds WHERE kind = $1 AND source = $2 AND enabled)
         WHERE kind = $1 AND source = $2",
    )
    .bind(kind)
    .bind(source)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
