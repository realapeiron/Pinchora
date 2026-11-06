use chrono::{DateTime, Utc};

use crate::{Db, locks::lock_users, sql_id};

/// Friends a player can have.
pub const MAX_FRIENDS: i64 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BefriendOutcome {
    Added,
    AlreadyFriends,
    /// This user has [`MAX_FRIENDS`] already.
    Full {
        user: u64,
    },
}

/// A friend as the list shows them.
#[derive(Debug, Clone)]
pub struct FriendRow {
    pub friend_id: u64,
    /// The name last remembered for them; empty if never seen.
    pub name: String,
    pub since: DateTime<Utc>,
    pub xp: i64,
    pub likes: i32,
    pub locked: bool,
}

type RawFriend = (i64, String, DateTime<Utc>, i64, i32, bool);

fn friend(raw: RawFriend) -> FriendRow {
    FriendRow {
        friend_id: raw.0 as u64,
        name: raw.1,
        since: raw.2,
        xp: raw.3,
        likes: raw.4,
        locked: raw.5,
    }
}

/// The friend query with `$tail` after the joins (a literal, so the SQL stays a static string).
macro_rules! friend_sql {
    ($tail:literal) => {
        concat!(
            "SELECT f.friend_id, COALESCE(n.name, ''), f.since, COALESCE(u.xp, 0),
                    COALESCE(h.likes, 0), COALESCE(h.locked, false)
             FROM friends f
             LEFT JOIN social_names n ON n.user_id = f.friend_id
             LEFT JOIN users u ON u.id = f.friend_id
             LEFT JOIN houses h ON h.user_id = f.friend_id ",
            $tail
        )
    };
}

impl Db {
    /// Makes two players friends, both ways at once, unless either has the most friends allowed.
    pub async fn befriend(&self, a: u64, b: u64) -> anyhow::Result<BefriendOutcome> {
        anyhow::ensure!(a != b, "nobody befriends themselves");
        let (low, high) = (sql_id(a.min(b)), sql_id(a.max(b)));
        let mut tx = self.pool().begin().await?;
        // Other pairings of either player wait here: the players' locks, taken in id order.
        lock_users(&mut tx, &[low, high]).await?;
        let (already,): (bool,) =
            sqlx::query_as("SELECT EXISTS (SELECT 1 FROM friends WHERE user_id = $1 AND friend_id = $2)")
                .bind(sql_id(a))
                .bind(sql_id(b))
                .fetch_one(&mut *tx)
                .await?;
        if already {
            return Ok(BefriendOutcome::AlreadyFriends);
        }
        for who in [a, b] {
            let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM friends WHERE user_id = $1")
                .bind(sql_id(who))
                .fetch_one(&mut *tx)
                .await?;
            if count >= MAX_FRIENDS {
                return Ok(BefriendOutcome::Full { user: who });
            }
        }
        sqlx::query("INSERT INTO friends (user_id, friend_id) VALUES ($1, $2), ($2, $1)")
            .bind(sql_id(a))
            .bind(sql_id(b))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(BefriendOutcome::Added)
    }

    /// Ends a friendship for both. False if they were not friends.
    pub async fn unfriend(&self, a: u64, b: u64) -> anyhow::Result<bool> {
        let removed = sqlx::query(
            "DELETE FROM friends WHERE (user_id = $1 AND friend_id = $2) OR (user_id = $2 AND friend_id = $1)",
        )
        .bind(sql_id(a))
        .bind(sql_id(b))
        .execute(self.pool())
        .await?
        .rows_affected();
        Ok(removed > 0)
    }

    pub async fn are_friends(&self, a: u64, b: u64) -> anyhow::Result<bool> {
        let (friends,): (bool,) =
            sqlx::query_as("SELECT EXISTS (SELECT 1 FROM friends WHERE user_id = $1 AND friend_id = $2)")
                .bind(sql_id(a))
                .bind(sql_id(b))
                .fetch_one(self.pool())
                .await?;
        Ok(friends)
    }

    pub async fn friend_count(&self, user: u64) -> anyhow::Result<i64> {
        let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM friends WHERE user_id = $1")
            .bind(sql_id(user))
            .fetch_one(self.pool())
            .await?;
        Ok(count)
    }

    /// A page of the player's friends, newest first, and how many they have in all.
    pub async fn friends(&self, user: u64, offset: i64, limit: i64) -> anyhow::Result<(Vec<FriendRow>, i64)> {
        let rows = sqlx::query_as::<_, RawFriend>(friend_sql!(
            "WHERE f.user_id = $1 ORDER BY f.since DESC, f.friend_id OFFSET $2 LIMIT $3"
        ))
        .bind(sql_id(user))
        .bind(offset)
        .bind(limit)
        .fetch_all(self.pool())
        .await?;
        Ok((rows.into_iter().map(friend).collect(), self.friend_count(user).await?))
    }

    /// Friends whose houses are open (not locked, not hidden), the most liked first: who `/house visit` offers.
    pub async fn visitable_friends(&self, user: u64, limit: i64) -> anyhow::Result<Vec<FriendRow>> {
        let rows = sqlx::query_as::<_, RawFriend>(friend_sql!(
            "WHERE f.user_id = $1 AND NOT COALESCE(h.locked, false)
             AND NOT EXISTS (SELECT 1 FROM user_privacy p WHERE p.user_id = f.friend_id AND p.hide_house)
             ORDER BY COALESCE(h.likes, 0) DESC, f.since DESC, f.friend_id LIMIT $2"
        ))
        .bind(sql_id(user))
        .bind(limit)
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(friend).collect())
    }
}
