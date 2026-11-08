//! What a player hides from others, and "delete my data".

use chrono::{DateTime, Duration, NaiveDate, Utc};

use crate::{
    Db,
    locks::{lock_users, retry, try_lock_users},
    sql_id,
};

/// Default retention for ordinary cooldowns. A live birthday expiry can retain the id longer,
/// through the annual claim window; no birthday date is kept in that tombstone.
pub const TOMBSTONE_DAYS: i64 = 35;

/// How long after deleting their data a pet cannot be taken for a walk: the walk's own cooldown, which
/// lives in the cache under the pet's id and so does not follow the player into a new pet.
const WALK_LOCK: Duration = Duration::minutes(45);

/// Every column that holds a player's id, as (table, column), children before the tables they
/// point at. "Delete my data" removes the player's rows from all of them, and a test reads the
/// schema to make sure a table added by a later migration is listed here (or in
/// [`OTHER_TABLES`], [`ID_LISTS`], [`CLEARED_COLUMNS`], [`DAILY_LIMITS`], [`ANONYMISED`] or [`ZEROED_COLUMNS`]) before it
/// can ship.
pub const USER_COLUMNS: &[(&str, &str)] = &[
    ("reef_aquarium", "user_id"),
    ("reef_requests", "user_id"),
    ("reef_catches", "user_id"),
    ("reef_season_claims", "user_id"),
    ("reef_partner_streaks", "user_a"),
    ("reef_partner_streaks", "user_b"),
    ("reef_loop", "user_id"),
    ("member_voice_levels", "user_id"),
    ("leveling_reaction_awards", "user_id"),
    ("leveling_departures", "user_id"),
    ("mod_booster_roles", "user_id"),
    ("mod_notes", "user_id"),
    ("mod_notes", "author_id"),
    ("mod_reports", "user_id"),
    ("mod_reports", "reporter_id"),
    ("mod_reports", "reviewer_id"),
    ("mod_appeals", "user_id"),
    ("mod_appeals", "reviewer_id"),
    ("mod_restrictions", "user_id"),
    ("mod_voice_bans", "user_id"),
    ("mod_report_blocks", "user_id"),
    ("qotd_questions", "suggested_by"),
    ("counting_stats", "user_id"),
    ("form_submissions", "user_id"),
    ("modmail_threads", "user_id"),
    ("modmail_blocks", "user_id"),
    ("modmail_pending", "user_id"),
    ("poll_extra_votes", "user_id"),
    ("personal_recurring_reminders", "user_id"),
    ("personal_checklist_items", "user_id"),
    ("keyword_highlights", "user_id"),
    ("member_anniversaries", "user_id"),
    ("anniversary_sent", "user_id"),
    ("confession_bans", "user_id"),
    ("confession_reports", "reporter_id"),
    ("confession_moderation", "moderator_id"),
    ("confession_reveals", "owner_id"),
    ("confessions", "author_id"),
    ("roblox_pending", "user_id"),
    ("roblox_links", "user_id"),
    ("roblox_sync", "user_id"),
    ("riot_links", "user_id"),
    ("tracker_cache", "user_id"),
    ("reef_layout_outbox", "owner_id"),
    ("reef_room_leases", "owner_id"),
    ("feed_channel_links", "user_id"),
    ("reef_consents", "user_id"),
    ("reef_chat_recipients", "user_id"),
    ("reef_reports", "reporter_id"),
    ("reef_chat_evidence", "sender_id"),
    ("reef_chat_evidence", "owner_id"),
    ("reef_sanctions", "owner_id"),
    ("reef_moderator_audit", "actor_id"),
    ("reef_moderator_audit", "target_user_id"),
    ("member_achievements", "user_id"),
    ("event_rsvps", "user_id"),
    ("event_reminder_deliveries", "user_id"),
    ("verification_pending", "user_id"),
    ("raid_joins", "user_id"),
    ("antinuke_events", "actor_id"),
    ("antinuke_events", "target_user_id"),
    ("antinuke_role_backups", "user_id"),
    ("antinuke_overwrites", "user_id"),
    ("birthdays", "user_id"),
    ("birthday_sent", "user_id"),
    ("birthday_gift_years", "user_id"),
    ("afk", "user_id"),
    ("automod_hits", "user_id"),
    ("automod_strikes", "user_id"),
    ("placements", "user_id"),
    ("rooms", "user_id"),
    ("houses", "user_id"),
    ("inventory", "user_id"),
    ("contest_votes", "voter_id"),
    ("contest_votes", "entry_id"),
    ("contest_pieces", "user_id"),
    ("contest_entries", "user_id"),
    ("guestbook_notes", "host_id"),
    ("guestbook_notes", "author_id"),
    ("guestbook_blocks", "host_id"),
    ("guestbook_blocks", "blocked_id"),
    ("visits", "host_id"),
    ("visits", "guest_id"),
    ("house_likes", "host_id"),
    ("house_likes", "liker_id"),
    ("gifts", "from_id"),
    ("gifts", "to_id"),
    ("stall_listings", "seller_id"),
    ("pets", "owner_id"),
    ("fish_book", "user_id"),
    ("fish_bucket", "user_id"),
    ("fish_milestones", "user_id"),
    ("fishing_gear", "user_id"),
    ("user_baits", "user_id"),
    ("user_rods", "user_id"),
    ("weekly_catch", "user_id"),
    ("activity_fish", "user_id"),
    ("user_backgrounds", "user_id"),
    ("user_badges", "user_id"),
    ("user_titles", "user_id"),
    ("partners", "user_a"),
    ("partners", "user_b"),
    ("friends", "user_id"),
    ("friends", "friend_id"),
    ("interactions", "from_user"),
    ("interactions", "to_user"),
    ("social_names", "user_id"),
    ("game_answers", "user_id"),
    ("game_stats", "user_id"),
    ("wordle_games", "user_id"),
    ("picross_games", "user_id"),
    ("solo_games", "user_id"),
    ("fleet_drafts", "user_id"),
    ("party_members", "user_id"),
    ("game_sessions", "host_id"),
    ("game_sessions", "guest_id"),
    ("party_votes", "voter_id"),
    // A vote for the player's entry (in Would You Rather `pick` is a side, 0 or 1, never an id).
    ("party_votes", "pick"),
    ("party_entries", "user_id"),
    ("party_players", "user_id"),
    ("party_games", "host_id"),
    ("dex_entries", "user_id"),
    ("collection_players", "user_id"),
    ("user_accessories", "user_id"),
    ("character_outfits", "user_id"),
    ("weekly_xp", "user_id"),
    ("members", "user_id"),
    // What roles a member held when they left, the roles that are theirs for a while, and the roles that wait for them.
    ("sticky_roles", "user_id"),
    ("timed_roles", "user_id"),
    ("delayed_roles", "user_id"),
    ("credit_ledger", "user_id"),
    ("reward_claims", "user_id"),
    ("reminders", "user_id"),
    ("reminder_optins", "user_id"),
    ("user_privacy", "user_id"),
    ("activity_monthly", "user_id"),
    ("giveaway_entries", "user_id"),
    ("giveaways", "host_id"),
    ("suggestion_votes", "user_id"),
    ("suggestions", "author_id"),
    ("poll_votes", "user_id"),
    ("polls", "author_id"),
    ("member_activity_daily", "user_id"),
    // The member's name on the transcripts of other people's tickets (the lines stay, as "Deleted user"),
    // then the tickets they opened, whose transcripts go with them.
    ("ticket_transcript_authors", "user_id"),
    ("tickets", "opener_id"),
    ("temp_voice_bans", "user_id"),
    ("starboard_stars", "user_id"),
    ("starboard_posts", "author_id"),
    ("invite_joins", "user_id"),
    ("invite_joins", "inviter_id"),
    ("invite_bonus", "user_id"),
    ("invite_role_grants", "user_id"),
    ("guild_invites", "inviter_id"),
    ("users", "id"),
];

/// How a player's id leaves a list of ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListEdit {
    /// Replaced by 0, for lists that run in step with another one (a podium keeps its places).
    Blank,
    /// Taken out.
    Remove,
}

/// Columns that hold a list of player ids, and what happens to the player's id in them.
pub const ID_LISTS: &[(&str, &str, ListEdit)] = &[
    ("verification", "exempt_users", ListEdit::Remove),
    ("antinuke", "trusted_users", ListEdit::Remove),
    ("contest_results", "winner_ids", ListEdit::Blank),
    ("giveaways", "winner_ids", ListEdit::Remove),
    // A critter drop's takers run in step with its cards.
    ("collection_drops", "takers", ListEdit::Blank),
    // Members the server log leaves alone.
    ("log_settings", "ignored_users", ListEdit::Remove),
];

/// Columns that name a player on rows that belong to somebody else and go on without them: the
/// column is set to NULL.
pub const CLEARED_COLUMNS: &[(&str, &str)] = &[
    ("form_submissions", "decided_by"),
    ("modmail_threads", "closed_by"),
    ("counting_channels", "last_user_id"),
    ("invite_rewards", "grantor_id"),
    ("raid_events", "ended_by"),
    ("sticky_messages", "updated_by"),
    ("scheduled_messages", "created_by"),
    ("suggestions", "decided_by"),
    ("confessions", "reviewer_id"),
    ("collection_drops", "host_id"),
    ("tickets", "claimed_by"),
    ("tickets", "closed_by"),
    ("events", "host_id"),
    ("automod_rules", "created_by"),
    // A room whose owner deleted their data has no owner: whoever is in it may claim it.
    ("temp_voice_channels", "owner_id"),
];

/// The games' per-day limits, as (table, player column, day column), the day in days since
/// 1970-01-01 (UTC): plays that pay, shells paid, shells lost at the tables. Today's rows stay when
/// the player deletes their data, so coming back picks the day up where it was (they hold no more
/// than a tombstone does: the id and how much of the day is used); older ones go, and today's go with
/// the games' own sweep of old days.
pub const DAILY_LIMITS: &[(&str, &str, &str)] = &[
    ("reef_loop_daily", "user_id", "day"),
    ("collection_daily", "user_id", "day"),
    ("party_daily", "user_id", "day"),
];

/// A server's own records that name the player who did something, as (table, id column, name
/// column): the record stays and the player leaves it, the id becoming 0 and the name
/// [`DELETED_NAME`].
///
/// The moderation records name the member they are about too, and that stays: a case is the server's
/// record of what happened, so a member cannot wipe their warns or their ban by deleting their data.
pub const ANONYMISED: &[(&str, &str, &str)] = &[
    ("dashboard_audit", "actor_id", "actor_name"),
    ("mod_cases", "moderator_id", "moderator_name"),
    ("staff_actions", "actor_id", "actor_name"),
];

/// The name a deleted player leaves on the records of [`ANONYMISED`].
pub const DELETED_NAME: &str = "Deleted user";

/// A server's own records that remember who made them, as (table, id column) and nothing else about
/// the player: the record stays (a server's custom commands outlive the manager who wrote them) and
/// the id becomes 0.
pub const ZEROED_COLUMNS: &[(&str, &str)] = &[
    ("reef_sanctions", "actor_id"),
    ("custom_commands", "created_by"),
    ("custom_responders", "created_by"),
    ("feeds", "created_by"),
];

/// Tables that belong to no player: server settings and shared stock.
pub const OTHER_TABLES: &[&str] = &[
    "dashboard_managers",
    "community_auto_channels",
    "anniversary_settings",
    "confession_settings",
    "community_autopost_jobs",
    "guild_leveling_parity",
    "guild_leveling_highlight_posts",
    "voice_level_roles",
    "mod_parity_settings",
    "qotd_settings",
    "counting_channels",
    "forms",
    "modmail_settings",
    "modmail_snippets",
    "roblox_settings",
    "roblox_binds",
    "roblox_cache",
    "tracker_settings",
    "patch_seen",
    // Minimal, expiring moderation tombstones: deletion/rejoin must not bypass a sanction.
    // Actor is zeroed and house-owned sanctions are deleted with the owner; no raw text retained.
    "reef_sanctions",
    "server_achievements",
    "event_settings",
    "events",
    "command_control",
    "command_permission_snapshots",
    "config_restore_points",
    "verification",
    "raid_settings",
    "raid_events",
    "antinuke",
    "antinuke_snapshots",
    "birthday_settings",
    "sticky_messages",
    "afk_reply_limits",
    "utility_settings",
    "sticky_copy_cleanup",
    "scheduled_messages",
    "automod_settings",
    "automod_rules",
    "automod_hit_actions",
    "_sqlx_migrations",
    "guilds",
    "level_roles",
    "market_stock",
    "rep_roles",
    "role_panel_roles",
    "role_panels",
    "auto_roles",
    "panel_reactions",
    "color_settings",
    "color_roles",
    "store_items",
    "xp_rules",
    "contest_results",
    // Kept on purpose, for [`TOMBSTONE_DAYS`]: see the migration.
    "deleted_users",
    "suggestion_settings",
    "collection_settings",
    // A channel's drops; who called one and who took its cards are cleared (see above).
    "collection_drops",
    "party_settings",
    "guild_modules",
    "mod_settings",
    "mod_case_counters",
    "mod_warn_thresholds",
    "log_channels",
    // The ignore list of members is cleaned (see `ID_LISTS`).
    "log_settings",
    "card_designs",
    "card_uploads",
    "guild_branding",
    // Stats: whole servers, channels and the bot, no player in them.
    "guild_activity_hourly",
    "channel_activity_daily",
    "counter_channels",
    "stats_flushes",
    "global_stats_daily",
    "invite_settings",
    "invite_rewards",
    "ticket_settings",
    "ticket_forms",
    "ticket_panels",
    "ticket_counters",
    // A closed ticket's lines; who wrote them is in `ticket_transcript_authors`.
    "ticket_transcripts",
    "feed_sources",
    "feed_items_seen",
    "feed_outbox",
    "feed_webhook_messages",
    "voice_creators",
    // A room belongs to the server; its owner is cleared (see above) and the members kept out of it go with the
    // player (`temp_voice_bans`).
    "temp_voice_channels",
    "starboards",
    "vote_channels",
    "starboard_deleted_sources",
    "starboard_copy_cleanup",
];

/// What a player hides.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, sqlx::FromRow)]
pub struct Privacy {
    pub hide_profile: bool,
    pub hide_leaderboard: bool,
    pub hide_house: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hide {
    Profile,
    Leaderboard,
    House,
}

impl Hide {
    fn column(self) -> &'static str {
        match self {
            Self::Profile => "hide_profile",
            Self::Leaderboard => "hide_leaderboard",
            Self::House => "hide_house",
        }
    }
}

/// What "delete my data" removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Deleted {
    pub rows: u64,
    /// In how many tables there was something to remove.
    pub tables: usize,
}

impl Db {
    pub async fn privacy(&self, user: u64) -> anyhow::Result<Privacy> {
        let row = sqlx::query_as::<_, Privacy>(
            "SELECT hide_profile, hide_leaderboard, hide_house FROM user_privacy WHERE user_id = $1",
        )
        .bind(sql_id(user))
        .fetch_optional(self.pool())
        .await?;
        Ok(row.unwrap_or_default())
    }

    /// Turns one choice on or off and returns all three as they are now.
    pub async fn set_privacy(&self, user: u64, which: Hide, on: bool) -> anyhow::Result<Privacy> {
        let column = which.column();
        // The column comes from the enum above, never from a caller's text.
        let sql = format!(
            "INSERT INTO user_privacy (user_id, {column}) VALUES ($1, $2)
             ON CONFLICT (user_id) DO UPDATE SET {column} = EXCLUDED.{column}
             RETURNING hide_profile, hide_leaderboard, hide_house"
        );
        Ok(sqlx::query_as::<_, Privacy>(sqlx::AssertSqlSafe(sql))
            .bind(sql_id(user))
            .bind(on)
            .fetch_one(self.pool())
            .await?)
    }

    /// Deletes every row about the player, in every table of [`USER_COLUMNS`], in one transaction:
    /// their account, house, pets, friends, notes they signed or were left, votes, history, all of
    /// it. What was held for them in the open goes first:
    ///
    /// - Shells they had put into a running duel go back to the other player, and the stakes other
    ///   racers put into a derby they host go back to those racers.
    /// - A giveaway they host is called off (its prize is part of the shells that go with them) and
    ///   they leave every giveaway's entries and winners.
    /// - Likes they gave are taken off the houses they went to.
    ///
    /// And a tombstone is left (see [`Db::walk_locked_until`] and the `deleted_users` migration) so
    /// that deleting is not a way to get a daily reward, a rep, a wordle or picross reward, paid
    /// visits, a free pet wake-up, a walk or a beachcomb a second time, and the games' limits of the
    /// day stay (see [`DAILY_LIMITS`]): the player comes back with nothing, but not with fresh
    /// cooldowns.
    ///
    /// The player's lock (see the `locks` module) is taken first and held to the commit, with the
    /// locks of the players it pays back or takes a like from, so a claim of theirs that overlaps the
    /// delete waits and then finds the tombstone instead of a row with no cooldowns. A delete that
    /// loses a deadlock anyway, or that finds a player it has to lock too late to wait for, runs
    /// again. Two players who are friends and delete at once can still cross on the rows they share
    /// (each deletes the other's visits and gifts); one of them loses and runs again.
    pub async fn delete_user_data(&self, user: u64, now: DateTime<Utc>) -> anyhow::Result<Deleted> {
        retry("delete_user_data", || async move {
            let mut tx = self.pool().begin().await?;
            let deleted = Self::delete_user_data_in(&mut tx, user, now).await?;
            tx.commit().await?;
            Ok(deleted)
        })
        .await
    }

    /// [`Db::delete_user_data`] up to, but not including, the commit: the caller owns the
    /// transaction. Tests use it to hold a delete open and watch what overlapping claims do.
    pub async fn delete_user_data_in(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        user: u64,
        now: DateTime<Utc>,
    ) -> anyhow::Result<Deleted> {
        let id = sql_id(user);
        let today = now.date_naive();
        // Who else gets written to is read before the locks. Somebody who came in between (a duel
        // accepted or a like given, both under the player's lock, which lets nobody new in once it
        // is held) is out of order by now: waiting for their lock could cross a transaction that
        // holds it and waits for one of ours. So their lock is only tried, and when somebody holds
        // it the delete gives up and starts over, reading the full set first.
        let mut locked = others_written(tx, id).await?;
        locked.push(id);
        lock_users(tx, &locked).await?;
        // A derby they host takes no new racer from here on (joining locks the lobby first).
        let hosted: Vec<(i64,)> = sqlx::query_as(
            "SELECT id FROM game_sessions WHERE host_id = $1 AND kind = 'derby' AND status = 'pending'
             ORDER BY id FOR UPDATE",
        )
        .bind(id)
        .fetch_all(&mut **tx)
        .await?;
        let late: Vec<i64> = others_written(tx, id)
            .await?
            .into_iter()
            .filter(|other| !locked.contains(other))
            .collect();
        if !late.is_empty() {
            try_lock_users(tx, &late).await?;
        }

        // Their own claims wait for this to finish, and see the tombstone when they go on.
        sqlx::query("SELECT 1 FROM users WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut **tx)
            .await?;
        leave_tombstone(tx, id, now, today).await?;

        let held: Vec<(i64, i64, Option<i64>, i64)> = sqlx::query_as(
            "SELECT id, host_id, guest_id, bet FROM game_sessions
             WHERE status = 'active' AND bet > 0 AND (host_id = $1 OR guest_id = $1)
             ORDER BY id FOR UPDATE",
        )
        .bind(id)
        .fetch_all(&mut **tx)
        .await?;
        for (_, host, guest, bet) in held {
            let other = if host == id { guest } else { Some(host) };
            if let Some(other) = other.filter(|o| *o != id) {
                sqlx::query("UPDATE users SET credits = credits + $2 WHERE id = $1")
                    .bind(other)
                    .bind(bet)
                    .execute(&mut **tx)
                    .await?;
                crate::economy::ledger(tx, other, bet, "duel_refund").await?;
            }
        }
        for (session,) in hosted {
            let racers: Vec<crate::PartyMember> = crate::party::members(tx, session)
                .await?
                .into_iter()
                .filter(|racer| racer.user_id != id)
                .collect();
            crate::party::refund_all(tx, session, &racers).await?;
        }
        // Today's limits stay; the days before go.
        let day = now.timestamp().div_euclid(86_400);
        for (table, column, day_column) in DAILY_LIMITS {
            let sql = format!("DELETE FROM {table} WHERE {column} = $1 AND {day_column} < $2");
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(id)
                .bind(day as i32)
                .execute(&mut **tx)
                .await?;
        }
        for (table, column, name) in ANONYMISED {
            let sql = format!("UPDATE {table} SET {column} = 0, {name} = $2 WHERE {column} = $1");
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(id)
                .bind(DELETED_NAME)
                .execute(&mut **tx)
                .await?;
        }

        for (table, column) in ZEROED_COLUMNS {
            let sql = format!("UPDATE {table} SET {column} = 0 WHERE {column} = $1");
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(id)
                .execute(&mut **tx)
                .await?;
        }

        // A guest's paid visits of the day stay counted when the host they went to is deleted, under
        // a number that is nobody's.
        sqlx::query("UPDATE visits SET host_id = -nextval('deleted_ids') WHERE host_id = $1 AND day >= $2")
            .bind(id)
            .bind(today)
            .execute(&mut **tx)
            .await?;
        // The likes they gave come off the houses they went to.
        sqlx::query(
            "UPDATE houses h SET likes = GREATEST(h.likes - l.n, 0)
             FROM (SELECT host_id, count(*)::INTEGER AS n FROM house_likes WHERE liker_id = $1 GROUP BY host_id) l
             WHERE h.user_id = l.host_id",
        )
        .bind(id)
        .execute(&mut **tx)
        .await?;

        for (table, column, edit) in ID_LISTS {
            let value = match edit {
                ListEdit::Blank => format!("array_replace({column}, $1, 0)"),
                ListEdit::Remove => format!("array_remove({column}, $1)"),
            };
            let sql = format!("UPDATE {table} SET {column} = {value} WHERE $1 = ANY({column})");
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(id)
                .execute(&mut **tx)
                .await?;
        }
        for (table, column) in CLEARED_COLUMNS {
            let sql = format!("UPDATE {table} SET {column} = NULL WHERE {column} = $1");
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(id)
                .execute(&mut **tx)
                .await?;
        }

        let mut deleted = Deleted::default();
        let mut touched: Vec<&str> = Vec::new();
        let affected_stars: Vec<(i64, i64)> =
            sqlx::query_as("SELECT starboard_id,message_id FROM starboard_stars WHERE user_id=$1")
                .bind(id)
                .fetch_all(&mut **tx)
                .await?;
        sqlx::query("INSERT INTO starboard_copy_cleanup(channel_id,copy_id) SELECT b.channel_id,p.board_message_id FROM starboard_posts p JOIN starboards b ON b.id=p.starboard_id WHERE p.author_id=$1 AND p.board_message_id>0 ON CONFLICT DO NOTHING")
            .bind(id).execute(&mut **tx).await?;
        sqlx::query("DELETE FROM roblox_cache WHERE key IN (SELECT 'groups:' || roblox_id::text FROM roblox_links WHERE user_id=$1)")
            .bind(sql_id(user)).execute(&mut **tx).await?;
        for (table, column) in USER_COLUMNS {
            // Table and column names are the constants above.
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!("DELETE FROM {table} WHERE {column} = $1")))
                .bind(id)
                .execute(&mut **tx)
                .await?
                .rows_affected();
            if rows > 0 {
                deleted.rows += rows;
                if !touched.contains(table) {
                    touched.push(table);
                }
            }
        }
        for (board, message) in affected_stars {
            sqlx::query("UPDATE starboard_posts SET star_count=(SELECT COALESCE(SUM(weight),0)::INT FROM starboard_stars WHERE starboard_id=$1 AND message_id=$2),sync_due=now(),sync_version=sync_version+1 WHERE starboard_id=$1 AND message_id=$2")
                .bind(board).bind(message).execute(&mut **tx).await?;
        }
        deleted.tables = touched.len();
        Ok(deleted)
    }

    /// Drops the tombstones of players who deleted their data more than [`TOMBSTONE_DAYS`] before
    /// `now`, except those with a live birthday expiry; returns how many went.
    pub async fn forget_old_tombstones(&self, now: DateTime<Utc>) -> anyhow::Result<u64> {
        let before = now - Duration::days(TOMBSTONE_DAYS);
        retry("forget_old_tombstones", || async move {
            // An annual birthday lock must not extend retention of unrelated cooldowns or ids.
            sqlx::query("UPDATE deleted_users SET deleted_at='epoch', last_daily_at=NULL, last_rep_at=NULL, weekly_at=NULL, monthly_at=NULL, day=NULL, visits=0, liked='{}', wordle_day=NULL, walk_until=NULL, pet_wake_day=NULL, picross_day=NULL, comb_ready_at=NULL WHERE user_id IN (SELECT user_id FROM deleted_users WHERE deleted_at<$1 AND deleted_at<>'epoch' AND birthday_until>$2 FOR UPDATE SKIP LOCKED)")
                .bind(before).bind(now).execute(self.pool()).await?;
            Ok(sqlx::query(
                "WITH old AS MATERIALIZED (
                     SELECT user_id FROM deleted_users WHERE deleted_at < $1 AND (birthday_until IS NULL OR birthday_until <= $2) FOR UPDATE SKIP LOCKED)
                 DELETE FROM deleted_users WHERE user_id IN (SELECT user_id FROM old)",
            )
            .bind(before)
            .bind(now)
            .execute(self.pool())
            .await?
            .rows_affected())
        })
        .await
    }

    /// Until when a player who deleted their data may not take a pet for a walk (a walk finds shells,
    /// and its cooldown is the pet's, not theirs). `None` when nothing holds them back. A delete of
    /// theirs that is still under way is waited for, so its tombstone is the one read.
    pub async fn walk_locked_until(&self, user: u64, now: DateTime<Utc>) -> anyhow::Result<Option<DateTime<Utc>>> {
        retry("walk_locked_until", || self.walk_locked_until_once(user, now)).await
    }

    /// One attempt of [`Db::walk_locked_until`].
    async fn walk_locked_until_once(&self, user: u64, now: DateTime<Utc>) -> anyhow::Result<Option<DateTime<Utc>>> {
        let id = sql_id(user);
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[id]).await?;
        let row: Option<(Option<DateTime<Utc>>,)> =
            sqlx::query_as("SELECT walk_until FROM deleted_users WHERE user_id = $1")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        tx.commit().await?;
        Ok(row.and_then(|(until,)| until).filter(|until| *until > now))
    }

    /// Whether the player had already played the daily wordle of `day` (days since 1970-01-01)
    /// before deleting their data, so the board they start again pays nothing. Waits for a delete
    /// of theirs that is still under way, like [`Db::walk_locked_until`].
    pub async fn daily_wordle_played_before_deleting(&self, user: u64, day: i32) -> anyhow::Result<bool> {
        retry("daily_wordle_played_before_deleting", || {
            self.daily_wordle_played_once(user, day)
        })
        .await
    }

    /// One attempt of [`Db::daily_wordle_played_before_deleting`].
    async fn daily_wordle_played_once(&self, user: u64, day: i32) -> anyhow::Result<bool> {
        Ok(self.tombstone_day(user, "wordle_day").await? >= Some(day))
    }

    /// Whether the player had already solved the daily picross of `day` (days since 1970-01-01)
    /// before deleting their data, so solving it again pays nothing. Waits for a delete of theirs that
    /// is still under way, like [`Db::walk_locked_until`].
    pub async fn daily_picross_solved_before_deleting(&self, user: u64, day: i32) -> anyhow::Result<bool> {
        retry("daily_picross_solved_before_deleting", || async move {
            Ok(self.tombstone_day(user, "picross_day").await? >= Some(day))
        })
        .await
    }

    /// A day column of the player's tombstone, read under their lock.
    async fn tombstone_day(&self, user: u64, column: &'static str) -> anyhow::Result<Option<i32>> {
        let id = sql_id(user);
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[id]).await?;
        // The column is one of the two names above, never a caller's text.
        let sql = format!("SELECT {column} FROM deleted_users WHERE user_id = $1");
        let row: Option<(Option<i32>,)> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(row.and_then(|(day,)| day))
    }
}

/// The players besides `id` whose rows a delete of `id` writes: the other player of each running
/// duel with shells in it (who gets their bet back), each racer with a stake in a derby `id` hosts
/// (who gets their stake back) and the host of each house `id` liked (whose house loses the like).
async fn others_written(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, id: i64) -> anyhow::Result<Vec<i64>> {
    let rows: Vec<(Option<i64>,)> = sqlx::query_as(
        "SELECT CASE WHEN host_id = $1 THEN guest_id ELSE host_id END FROM game_sessions
         WHERE status = 'active' AND bet > 0 AND (host_id = $1 OR guest_id = $1)
         UNION SELECT m.user_id FROM party_members m JOIN game_sessions s ON s.id = m.session_id
         WHERE s.host_id = $1 AND s.kind = 'derby' AND s.status = 'pending' AND m.amount > 0
         UNION SELECT host_id FROM house_likes WHERE liker_id = $1
         UNION SELECT s.user_id FROM starboard_stars s JOIN starboard_posts p
             ON p.starboard_id=s.starboard_id AND p.message_id=s.message_id WHERE p.author_id=$1
         UNION SELECT p.author_id FROM starboard_posts p JOIN starboard_stars s
             ON s.starboard_id=p.starboard_id AND s.message_id=p.message_id WHERE s.user_id=$1
         UNION SELECT r.user_id FROM reef_chat_recipients r JOIN reef_chat_evidence e ON e.id=r.message_id
             WHERE e.sender_id=$1 OR e.owner_id=$1
         UNION SELECT r.reporter_id FROM reef_reports r JOIN reef_chat_evidence e ON e.id=r.message_id
             WHERE e.sender_id=$1 OR e.owner_id=$1
         UNION SELECT e.sender_id FROM reef_chat_evidence e WHERE e.owner_id=$1
         UNION SELECT e.owner_id FROM reef_chat_evidence e WHERE e.sender_id=$1",
    )
    .bind(id)
    .fetch_all(&mut **tx)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(other,)| other)
        .filter(|other| *other != id)
        .collect())
}

/// What the player had used up, as it stands in their rows.
#[derive(sqlx::FromRow)]
struct Used {
    last_daily_at: Option<DateTime<Utc>>,
    last_rep_at: Option<DateTime<Utc>>,
    weekly_at: Option<DateTime<Utc>>,
    monthly_at: Option<DateTime<Utc>>,
    visits: i64,
    liked: Vec<i64>,
    wordle_day: Option<i32>,
    picross_day: Option<i32>,
    pet_wake_day: Option<i32>,
    comb_ready_at: Option<DateTime<Utc>>,
    has_pet: bool,
}

/// Writes (or brings up to date) the tombstone of a player about to be deleted. Even a player
/// with no cooldowns needs one to reject a sealed stats batch. Times only ever move forward.
async fn leave_tombstone(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: i64,
    now: DateTime<Utc>,
    today: NaiveDate,
) -> anyhow::Result<()> {
    let used: Used = sqlx::query_as(
        "SELECT (SELECT last_daily_at FROM users WHERE id = $1) AS last_daily_at,
                (SELECT last_rep_at FROM users WHERE id = $1) AS last_rep_at,
                (SELECT claimed_at FROM reward_claims WHERE user_id = $1 AND kind = 'weekly') AS weekly_at,
                (SELECT claimed_at FROM reward_claims WHERE user_id = $1 AND kind = 'monthly') AS monthly_at,
                (SELECT count(*) FROM visits WHERE guest_id = $1 AND day = $2) AS visits,
                COALESCE((SELECT array_agg(host_id) FROM house_likes WHERE liker_id = $1 AND day = $2), '{}'::BIGINT[]) AS liked,
                (SELECT max(day) FROM wordle_games WHERE user_id = $1 AND mode IN ('daily', 'daily-tr')) AS wordle_day,
                (SELECT max(day) FROM picross_games WHERE user_id = $1 AND mode = 'daily' AND status = 'solved')
                    AS picross_day,
                (SELECT pet_wake_day FROM users WHERE id = $1) AS pet_wake_day,
                (SELECT comb_ready_at FROM collection_players WHERE user_id = $1 AND comb_ready_at > $3)
                    AS comb_ready_at,
                EXISTS (SELECT 1 FROM pets WHERE owner_id = $1) AS has_pet",
    )
    .bind(id)
    .bind(today)
    .bind(now)
    .fetch_one(&mut **tx)
    .await?;
    // Stats batches may already be in flight even for a player with no cooldowns.
    sqlx::query(
        "INSERT INTO deleted_users
             (user_id, deleted_at, last_daily_at, last_rep_at, weekly_at, monthly_at, day, visits, liked, wordle_day,
              walk_until, pet_wake_day, picross_day, comb_ready_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
         ON CONFLICT (user_id) DO UPDATE SET
             deleted_at = EXCLUDED.deleted_at,
             last_daily_at = GREATEST(deleted_users.last_daily_at, EXCLUDED.last_daily_at),
             last_rep_at = GREATEST(deleted_users.last_rep_at, EXCLUDED.last_rep_at),
             weekly_at = GREATEST(deleted_users.weekly_at, EXCLUDED.weekly_at),
             monthly_at = GREATEST(deleted_users.monthly_at, EXCLUDED.monthly_at),
             visits = CASE WHEN deleted_users.day = EXCLUDED.day THEN deleted_users.visits ELSE 0 END
                      + EXCLUDED.visits,
             liked = CASE WHEN deleted_users.day = EXCLUDED.day
                          THEN ARRAY(SELECT DISTINCT unnest(deleted_users.liked || EXCLUDED.liked))
                          ELSE EXCLUDED.liked END,
             day = EXCLUDED.day,
             wordle_day = GREATEST(deleted_users.wordle_day, EXCLUDED.wordle_day),
             walk_until = GREATEST(deleted_users.walk_until, EXCLUDED.walk_until),
             pet_wake_day = GREATEST(deleted_users.pet_wake_day, EXCLUDED.pet_wake_day),
             picross_day = GREATEST(deleted_users.picross_day, EXCLUDED.picross_day),
             comb_ready_at = GREATEST(deleted_users.comb_ready_at, EXCLUDED.comb_ready_at)",
    )
    .bind(id)
    .bind(now)
    .bind(used.last_daily_at)
    .bind(used.last_rep_at)
    .bind(used.weekly_at)
    .bind(used.monthly_at)
    .bind(today)
    .bind(used.visits)
    .bind(&used.liked)
    .bind(used.wordle_day)
    .bind(used.has_pet.then_some(now + WALK_LOCK))
    .bind(used.pet_wake_day)
    .bind(used.picross_day)
    .bind(used.comb_ready_at)
    .execute(&mut **tx)
    .await?;
    // Preserve only an expiry, not the erased birthday or server membership. A claimed celebration
    // stays spent until the next UTC year; an unclaimed date keeps its original 30-day change lock.
    sqlx::query("UPDATE deleted_users SET birthday_until=GREATEST(birthday_until, (SELECT max(locked_until) FROM birthday_gift_years WHERE user_id=$1), (SELECT make_date(max(utc_year)::int+1,1,1)::timestamp AT TIME ZONE 'UTC' FROM birthday_sent WHERE user_id=$1)) WHERE user_id=$1")
        .bind(id).execute(&mut **tx).await?;
    Ok(())
}
