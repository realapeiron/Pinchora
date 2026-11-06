//! What a player hides from others, and "delete my data".

use crate::{Db, sql_id};

/// Every column that holds a player's id, as (table, column), children before the tables they
/// point at. "Delete my data" removes the player's rows from all of them, and a test reads the
/// schema to make sure a table added by a later migration is listed here (or in
/// [`OTHER_TABLES`]) before it can ship.
pub const USER_COLUMNS: &[(&str, &str)] = &[
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
    ("game_sessions", "host_id"),
    ("game_sessions", "guest_id"),
    ("weekly_xp", "user_id"),
    ("members", "user_id"),
    ("credit_ledger", "user_id"),
    ("reward_claims", "user_id"),
    ("reminders", "user_id"),
    ("reminder_optins", "user_id"),
    ("user_privacy", "user_id"),
    ("activity_monthly", "user_id"),
    ("users", "id"),
];

/// Columns that hold a list of player ids; the player's id is replaced by 0 in them (a podium keeps
/// its places).
pub const ID_LISTS: &[(&str, &str)] = &[("contest_results", "winner_ids")];

/// Tables that belong to no player: server settings and shared stock.
pub const OTHER_TABLES: &[&str] = &[
    "_sqlx_migrations",
    "guilds",
    "level_roles",
    "market_stock",
    "rep_roles",
    "role_panel_roles",
    "role_panels",
    "store_items",
    "xp_rules",
    "contest_results",
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
    /// it. Shells they had put into a running duel go back to the other player first, so nobody
    /// loses a wager over somebody else's deletion.
    pub async fn delete_user_data(&self, user: u64) -> anyhow::Result<Deleted> {
        let id = sql_id(user);
        let mut tx = self.pool().begin().await?;

        let held: Vec<(i64, i64, Option<i64>, i64)> = sqlx::query_as(
            "SELECT id, host_id, guest_id, bet FROM game_sessions
             WHERE status = 'active' AND bet > 0 AND (host_id = $1 OR guest_id = $1)
             ORDER BY id FOR UPDATE",
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;
        for (_, host, guest, bet) in held {
            let other = if host == id { guest } else { Some(host) };
            if let Some(other) = other.filter(|o| *o != id) {
                sqlx::query("UPDATE users SET credits = credits + $2 WHERE id = $1")
                    .bind(other)
                    .bind(bet)
                    .execute(&mut *tx)
                    .await?;
                crate::economy::ledger(&mut tx, other, bet, "duel_refund").await?;
            }
        }

        for (table, column) in ID_LISTS {
            // The lists are always as long as their votes; only the id is blanked.
            let sql = format!("UPDATE {table} SET {column} = array_replace({column}, $1, 0) WHERE $1 = ANY({column})");
            sqlx::query(sqlx::AssertSqlSafe(sql)).bind(id).execute(&mut *tx).await?;
        }

        let mut deleted = Deleted::default();
        let mut touched: Vec<&str> = Vec::new();
        for (table, column) in USER_COLUMNS {
            // Table and column names are the constants above.
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!("DELETE FROM {table} WHERE {column} = $1")))
                .bind(id)
                .execute(&mut *tx)
                .await?
                .rows_affected();
            if rows > 0 {
                deleted.rows += rows;
                if !touched.contains(table) {
                    touched.push(table);
                }
            }
        }
        deleted.tables = touched.len();
        tx.commit().await?;
        Ok(deleted)
    }
}
