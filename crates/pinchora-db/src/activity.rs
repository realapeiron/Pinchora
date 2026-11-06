//! What a player did each calendar month, counted from the places it happens, for `/wrapped`.

use chrono::{Datelike, NaiveDate};
use sqlx::PgConnection;

use crate::{Db, sql_id};

/// A number `/wrapped` keeps per month.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Counter {
    /// Chat messages that earned XP.
    Messages,
    /// Server XP earned in chat and voice, in every server.
    Xp,
    /// Shells earned (not received from another player, not refunded).
    Shells,
    /// Fish caught.
    Fish,
    /// Houses visited (the first visit to a house each day).
    Visits,
    LikesGiven,
    LikesReceived,
    /// Care actions on a pet: feeding, playing, washing, walking.
    PetCare,
}

impl Counter {
    fn column(self) -> &'static str {
        match self {
            Self::Messages => "messages",
            Self::Xp => "xp",
            Self::Shells => "shells",
            Self::Fish => "fish",
            Self::Visits => "visits",
            Self::LikesGiven => "likes_given",
            Self::LikesReceived => "likes_received",
            Self::PetCare => "pet_care",
        }
    }
}

/// One month of a player's numbers, or a year of them added up.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, sqlx::FromRow)]
pub struct Activity {
    pub messages: i64,
    pub xp: i64,
    pub shells: i64,
    pub fish: i64,
    pub visits: i64,
    pub likes_given: i64,
    pub likes_received: i64,
    pub pet_care: i64,
}

impl Activity {
    /// Everything counted in `self` and `other`.
    pub fn plus(self, other: Self) -> Self {
        Self {
            messages: self.messages + other.messages,
            xp: self.xp + other.xp,
            shells: self.shells + other.shells,
            fish: self.fish + other.fish,
            visits: self.visits + other.visits,
            likes_given: self.likes_given + other.likes_given,
            likes_received: self.likes_received + other.likes_received,
            pet_care: self.pet_care + other.pet_care,
        }
    }

    /// How busy the month was: the things done, not the amounts of XP and shells they earned.
    pub fn actions(&self) -> i64 {
        self.messages + self.fish + self.visits + self.likes_given + self.likes_received + self.pet_care
    }
}

#[derive(sqlx::FromRow)]
struct MonthRow {
    month: NaiveDate,
    #[sqlx(flatten)]
    activity: Activity,
}

/// The shells of a ledger entry that the player earned: money, not a move of shells between players
/// and not a bet or a purchase coming back. A duel's pot pays the winner both stakes, one of them
/// their own, so only half of it was won.
pub(crate) fn earned(delta: i64, reason: &str) -> i64 {
    if delta <= 0 || reason == "transfer_in" || reason.ends_with("refund") {
        0
    } else if reason == "duel_pot" {
        delta / 2
    } else {
        delta
    }
}

/// Adds `n` to the player's `counter` for the month of `month` (this month when `None`).
pub(crate) async fn bump(
    conn: &mut PgConnection,
    user: i64,
    counter: Counter,
    n: i64,
    month: Option<NaiveDate>,
) -> anyhow::Result<()> {
    if n == 0 {
        return Ok(());
    }
    let column = counter.column();
    // The column comes from the enum above, never from a caller's text.
    let sql = format!(
        "INSERT INTO activity_monthly (user_id, month, {column})
         VALUES ($1, COALESCE($2, date_trunc('month', now() AT TIME ZONE 'UTC')::date), $3)
         ON CONFLICT (user_id, month) DO UPDATE SET {column} = activity_monthly.{column} + EXCLUDED.{column}"
    );
    sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(user)
        .bind(month)
        .bind(n)
        .execute(conn)
        .await?;
    Ok(())
}

impl Db {
    /// Counts `n` of `counter` for this month, for the places that run outside a transaction.
    pub async fn bump_activity(&self, user: u64, counter: Counter, n: i64) -> anyhow::Result<()> {
        let mut conn = self.pool().acquire().await?;
        bump(&mut conn, sql_id(user), counter, n, None).await
    }

    /// Counts `n` of `counter` in the month that `month` falls in (its first day is kept).
    pub async fn add_activity_in(&self, user: u64, month: NaiveDate, counter: Counter, n: i64) -> anyhow::Result<()> {
        let first = month.with_day(1).unwrap_or(month);
        let mut conn = self.pool().acquire().await?;
        bump(&mut conn, sql_id(user), counter, n, Some(first)).await
    }

    /// The player's numbers for each month of `year` that has any, as (month 1-12, numbers).
    pub async fn activity_year(&self, user: u64, year: i32) -> anyhow::Result<Vec<(u32, Activity)>> {
        let rows: Vec<MonthRow> = sqlx::query_as(
            "SELECT month, messages, xp, shells, fish, visits, likes_given, likes_received, pet_care
             FROM activity_monthly
             WHERE user_id = $1 AND month >= make_date($2, 1, 1) AND month < make_date($2 + 1, 1, 1)
             ORDER BY month",
        )
        .bind(sql_id(user))
        .bind(year)
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|r| (r.month.month(), r.activity)).collect())
    }

    /// Counts a catch of `fish_id` in `year`.
    pub async fn add_catch_in(&self, user: u64, year: i32, fish_id: &str) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO activity_fish (user_id, year, fish_id, caught) VALUES ($1, $2, $3, 1)
             ON CONFLICT (user_id, year, fish_id) DO UPDATE SET caught = activity_fish.caught + 1",
        )
        .bind(sql_id(user))
        .bind(year)
        .bind(fish_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// The fish the player caught most in `year`, with how many: ties go to the one with the lower id.
    pub async fn favourite_fish(&self, user: u64, year: i32) -> anyhow::Result<Option<(String, i64)>> {
        let row: Option<(String, i32)> = sqlx::query_as(
            "SELECT fish_id, caught FROM activity_fish WHERE user_id = $1 AND year = $2 AND caught > 0
             ORDER BY caught DESC, fish_id LIMIT 1",
        )
        .bind(sql_id(user))
        .bind(year)
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|(fish, caught)| (fish, i64::from(caught))))
    }

    /// The house the player went to on most days of `year`: the host, the number of days and whether
    /// the host is a friend. A friend's house beats anyone else's, however often.
    pub async fn most_visited_house(&self, user: u64, year: i32) -> anyhow::Result<Option<(u64, i64, bool)>> {
        let row: Option<(i64, i64, bool)> = sqlx::query_as(
            "SELECT v.host_id, count(*), bool_or(f.friend_id IS NOT NULL)
             FROM visits v
             LEFT JOIN friends f ON f.user_id = $1 AND f.friend_id = v.host_id
             WHERE v.guest_id = $1 AND v.host_id > 0 AND v.host_id <> $1
               AND v.day >= make_date($2, 1, 1) AND v.day < make_date($2 + 1, 1, 1)
             GROUP BY v.host_id
             ORDER BY bool_or(f.friend_id IS NOT NULL) DESC, count(*) DESC, max(v.day) DESC, v.host_id
             LIMIT 1",
        )
        .bind(sql_id(user))
        .bind(year)
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|(host, visits, friend)| (host as u64, visits, friend)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_real_earnings_count_as_shells_earned() {
        assert_eq!(earned(250, "daily"), 250);
        assert_eq!(earned(30, "visit"), 30);
        assert_eq!(earned(-30, "market"), 0);
        assert_eq!(earned(100, "transfer_in"), 0);
        assert_eq!(earned(60, "duel_refund"), 0);
        assert_eq!(earned(1_500, "pet_capsule_refund"), 0);
        assert_eq!(earned(200, "duel_pot"), 100, "the winner's own stake is not a win");
        assert_eq!(earned(40, "duel_win"), 40, "the tic-tac-toe reward is all won");
    }
}
