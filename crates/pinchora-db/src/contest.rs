use chrono::{DateTime, Utc};

use crate::{
    Db, PlacementRow,
    locks::{lock_users, retry},
    sql_id,
};

/// A room entered in a server's weekly contest, with its votes so far.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ContestEntry {
    pub user_id: i64,
    pub owner_name: String,
    pub room_idx: i16,
    pub tier: i16,
    pub room_type: String,
    pub wallpaper: String,
    pub floor: String,
    pub entered_at: DateTime<Utc>,
    pub votes: i64,
}

/// A finished contest week: up to three winners, best first.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ContestResult {
    pub week: i32,
    pub winner_ids: Vec<i64>,
    pub winner_votes: Vec<i32>,
    pub entries: i32,
}

/// `SELECT` of [`ContestEntry`] rows from `contest_entries e`, followed by `$tail`.
macro_rules! entries {
    ($tail:literal) => {
        concat!(
            "SELECT e.user_id, e.owner_name, e.room_idx, e.tier, e.room_type, e.wallpaper, e.floor, e.entered_at,
                 (SELECT count(*) FROM contest_votes v
                  WHERE v.guild_id = e.guild_id AND v.week = e.week AND v.entry_id = e.user_id) AS votes
             FROM contest_entries e ",
            $tail
        )
    };
}

/// Entries `$3` may still vote on: not theirs and not voted on yet.
macro_rules! open_to {
    () => {
        "WHERE e.guild_id = $1 AND e.week = $2 AND e.user_id <> $3
         AND NOT EXISTS (SELECT 1 FROM contest_votes v WHERE v.guild_id = e.guild_id AND v.week = e.week
                         AND v.entry_id = e.user_id AND v.voter_id = $3) "
    };
}

impl Db {
    /// Enters a room in this week's contest, or replaces the entry's snapshot with the room as
    /// it is now (votes stay). Returns how many pieces the snapshot has; nothing is saved for
    /// an empty room.
    pub async fn enter_contest(
        &self,
        guild: u64,
        week: i32,
        user: u64,
        owner_name: &str,
        room_idx: i16,
    ) -> anyhow::Result<u64> {
        retry("enter_contest", || {
            self.enter_contest_once(guild, week, user, owner_name, room_idx)
        })
        .await
    }

    /// One attempt of [`Db::enter_contest`].
    async fn enter_contest_once(
        &self,
        guild: u64,
        week: i32,
        user: u64,
        owner_name: &str,
        room_idx: i16,
    ) -> anyhow::Result<u64> {
        let (guild, user) = (sql_id(guild), sql_id(user));
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[user]).await?;
        sqlx::query(
            "INSERT INTO contest_entries (guild_id, week, user_id, owner_name, room_idx, tier, room_type, wallpaper, floor)
             SELECT $1, $2, $3, $4, r.idx, h.tier, r.room_type, r.wallpaper, r.floor
             FROM rooms r JOIN houses h ON h.user_id = r.user_id
             WHERE r.user_id = $3 AND r.idx = $5
             ON CONFLICT (guild_id, week, user_id) DO UPDATE SET
                 owner_name = EXCLUDED.owner_name, room_idx = EXCLUDED.room_idx, tier = EXCLUDED.tier,
                 room_type = EXCLUDED.room_type, wallpaper = EXCLUDED.wallpaper, floor = EXCLUDED.floor",
        )
        .bind(guild)
        .bind(week)
        .bind(user)
        .bind(owner_name)
        .bind(room_idx)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM contest_pieces WHERE guild_id = $1 AND week = $2 AND user_id = $3")
            .bind(guild)
            .bind(week)
            .bind(user)
            .execute(&mut *tx)
            .await?;
        let pieces = sqlx::query(
            "INSERT INTO contest_pieces (guild_id, week, user_id, n, item_id, x, y, flipped)
             SELECT $1, $2, $3, row_number() OVER (ORDER BY id), item_id, x, y, flipped
             FROM placements WHERE user_id = $3 AND room_idx = $4",
        )
        .bind(guild)
        .bind(week)
        .bind(user)
        .bind(room_idx)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if pieces > 0 {
            tx.commit().await?;
        }
        Ok(pieces)
    }

    pub async fn contest_entry(&self, guild: u64, week: i32, user: u64) -> anyhow::Result<Option<ContestEntry>> {
        Ok(
            sqlx::query_as::<_, ContestEntry>(entries!("WHERE e.guild_id = $1 AND e.week = $2 AND e.user_id = $3"))
                .bind(sql_id(guild))
                .bind(week)
                .bind(sql_id(user))
                .fetch_optional(self.pool())
                .await?,
        )
    }

    /// The entries of these players, in no particular order.
    pub async fn contest_entries_of(&self, guild: u64, week: i32, users: &[i64]) -> anyhow::Result<Vec<ContestEntry>> {
        Ok(sqlx::query_as::<_, ContestEntry>(entries!(
            "WHERE e.guild_id = $1 AND e.week = $2 AND e.user_id = ANY($3)"
        ))
        .bind(sql_id(guild))
        .bind(week)
        .bind(users)
        .fetch_all(self.pool())
        .await?)
    }

    /// An entry's furniture as it was entered, ready to draw.
    pub async fn contest_pieces(&self, guild: u64, week: i32, user: u64) -> anyhow::Result<Vec<PlacementRow>> {
        Ok(sqlx::query_as::<_, PlacementRow>(
            "SELECT p.n::BIGINT AS id, e.room_idx, p.item_id, p.x, p.y, p.flipped
             FROM contest_pieces p JOIN contest_entries e USING (guild_id, week, user_id)
             WHERE p.guild_id = $1 AND p.week = $2 AND p.user_id = $3 ORDER BY p.n",
        )
        .bind(sql_id(guild))
        .bind(week)
        .bind(sql_id(user))
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn contest_entry_count(&self, guild: u64, week: i32) -> anyhow::Result<i64> {
        let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM contest_entries WHERE guild_id = $1 AND week = $2")
            .bind(sql_id(guild))
            .bind(week)
            .fetch_one(self.pool())
            .await?;
        Ok(n)
    }

    /// A random entry the voter has not voted on and that is not theirs, `skip` only when no
    /// other is left; with how many such entries there are.
    pub async fn next_ballot(
        &self,
        guild: u64,
        week: i32,
        voter: u64,
        skip: Option<u64>,
    ) -> anyhow::Result<(Option<ContestEntry>, i64)> {
        let entry = sqlx::query_as::<_, ContestEntry>(concat!(
            entries!(""),
            open_to!(),
            "ORDER BY e.user_id = $4, random() LIMIT 1"
        ))
        .bind(sql_id(guild))
        .bind(week)
        .bind(sql_id(voter))
        .bind(skip.map_or(0, sql_id))
        .fetch_optional(self.pool())
        .await?;
        let (left,): (i64,) = sqlx::query_as(concat!("SELECT count(*) FROM contest_entries e ", open_to!()))
            .bind(sql_id(guild))
            .bind(week)
            .bind(sql_id(voter))
            .fetch_one(self.pool())
            .await?;
        Ok((entry, left))
    }

    /// One vote per entry per voter, never for your own; false when it was not counted.
    pub async fn contest_vote(&self, guild: u64, week: i32, entry: u64, voter: u64) -> anyhow::Result<bool> {
        let n = sqlx::query(
            "INSERT INTO contest_votes (guild_id, week, entry_id, voter_id)
             SELECT $1, $2, $3, $4 WHERE $3 <> $4 AND EXISTS (
                 SELECT 1 FROM contest_entries WHERE guild_id = $1 AND week = $2 AND user_id = $3)
             ON CONFLICT DO NOTHING",
        )
        .bind(sql_id(guild))
        .bind(week)
        .bind(sql_id(entry))
        .bind(sql_id(voter))
        .execute(self.pool())
        .await?
        .rows_affected();
        Ok(n == 1)
    }

    /// Closes every week of the server before `current` that has entries but no result: the
    /// top entries with at least one vote (most votes, then the earlier entry) win `prizes`
    /// through the ledger, and the first gets `title`. Writing the result row is what makes it
    /// happen once, however many calls race. Returns the weeks closed now.
    pub async fn close_contest_weeks(
        &self,
        guild: u64,
        current: i32,
        prizes: &[i64],
        title: &str,
    ) -> anyhow::Result<Vec<i32>> {
        let guild = sql_id(guild);
        let open: Vec<(i32,)> = sqlx::query_as(
            "SELECT DISTINCT e.week FROM contest_entries e
             WHERE e.guild_id = $1 AND e.week < $2 AND NOT EXISTS (
                 SELECT 1 FROM contest_results r WHERE r.guild_id = e.guild_id AND r.week = e.week)
             ORDER BY e.week",
        )
        .bind(guild)
        .bind(current)
        .fetch_all(self.pool())
        .await?;
        let mut closed = Vec::new();
        for (week,) in open {
            if retry("close_contest_week", || {
                self.close_contest_week_once(guild, week, prizes, title)
            })
            .await?
            {
                closed.push(week);
            }
        }
        Ok(closed)
    }

    /// One attempt of closing one week of [`Db::close_contest_weeks`]; false when it was closed
    /// already.
    async fn close_contest_week_once(
        &self,
        guild: i64,
        week: i32,
        prizes: &[i64],
        title: &str,
    ) -> anyhow::Result<bool> {
        let mut tx = self.pool().begin().await?;
        let standings: Vec<(i64, i64)> = sqlx::query_as(
            "SELECT e.user_id, count(v.voter_id) FROM contest_entries e
             LEFT JOIN contest_votes v ON v.guild_id = e.guild_id AND v.week = e.week AND v.entry_id = e.user_id
             WHERE e.guild_id = $1 AND e.week = $2
             GROUP BY e.user_id, e.entered_at
             ORDER BY count(v.voter_id) DESC, e.entered_at, e.user_id",
        )
        .bind(guild)
        .bind(week)
        .fetch_all(&mut *tx)
        .await?;
        let winners: Vec<(i64, i64)> = standings
            .iter()
            .copied()
            .filter(|(_, votes)| *votes > 0)
            .take(prizes.len())
            .collect();
        // The winners' locks before anything is written (see the `locks` module): the standings
        // of a week that is over do not move.
        lock_users(&mut tx, &winners.iter().map(|(user, _)| *user).collect::<Vec<i64>>()).await?;
        let fresh = sqlx::query(
            "INSERT INTO contest_results (guild_id, week, winner_ids, winner_votes, entries)
             VALUES ($1, $2, $3, $4, $5) ON CONFLICT (guild_id, week) DO NOTHING",
        )
        .bind(guild)
        .bind(week)
        .bind(winners.iter().map(|(u, _)| *u).collect::<Vec<i64>>())
        .bind(winners.iter().map(|(_, v)| *v as i32).collect::<Vec<i32>>())
        .bind(standings.len() as i32)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            == 1;
        if !fresh {
            return Ok(false);
        }
        for (&(user, _), &prize) in winners.iter().zip(prizes) {
            sqlx::query(
                "INSERT INTO users (id, credits) VALUES ($1, $2)
                 ON CONFLICT (id) DO UPDATE SET credits = users.credits + EXCLUDED.credits",
            )
            .bind(user)
            .bind(prize)
            .execute(&mut *tx)
            .await?;
            crate::economy::ledger(&mut tx, user, prize, "contest_prize").await?;
        }
        if let Some(&(first, _)) = winners.first() {
            sqlx::query("INSERT INTO user_titles (user_id, title_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
                .bind(first)
                .bind(title)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(true)
    }

    /// The newest finished week before `before`.
    pub async fn last_contest_result(&self, guild: u64, before: i32) -> anyhow::Result<Option<ContestResult>> {
        Ok(sqlx::query_as::<_, ContestResult>(
            "SELECT week, winner_ids, winner_votes, entries FROM contest_results
             WHERE guild_id = $1 AND week < $2 ORDER BY week DESC LIMIT 1",
        )
        .bind(sql_id(guild))
        .bind(before)
        .fetch_optional(self.pool())
        .await?)
    }
}
