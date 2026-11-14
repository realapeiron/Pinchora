use super::*;

impl Db {
    /// A vote in a would-you-rather poll for side 0 or 1; changing it is allowed until the poll closes.
    pub async fn put_poll_vote(
        &self,
        id: i64,
        voter: u64,
        side: i64,
        now: DateTime<Utc>,
    ) -> anyhow::Result<PollChoice> {
        retry("put_poll_vote", || self.put_poll_vote_once(id, voter, side, now)).await
    }

    /// One attempt of [`Db::put_poll_vote`].
    async fn put_poll_vote_once(
        &self,
        id: i64,
        voter: u64,
        side: i64,
        now: DateTime<Utc>,
    ) -> anyhow::Result<PollChoice> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(voter)]).await?;
        let Some(game) = lock_game(&mut tx, id).await? else {
            return Ok(PollChoice::Closed);
        };
        if !matches!(game.kind.as_str(), "wouldyourrather" | "nhie") || !game.is_playing() || game.late(now) {
            return Ok(PollChoice::Closed);
        }
        let previous: Option<(i64,)> =
            sqlx::query_as("SELECT pick FROM party_votes WHERE game_id = $1 AND round = 0 AND voter_id = $2")
                .bind(id)
                .bind(sql_id(voter))
                .fetch_optional(&mut *tx)
                .await?;
        if previous.is_some_and(|(pick,)| pick == side) {
            return Ok(PollChoice::Same);
        }
        sqlx::query(
            "INSERT INTO party_votes (game_id, round, voter_id, pick) VALUES ($1, 0, $2, $3)
             ON CONFLICT (game_id, round, voter_id) DO UPDATE SET pick = EXCLUDED.pick",
        )
        .bind(id)
        .bind(sql_id(voter))
        .bind(side)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(if previous.is_some() {
            PollChoice::Changed
        } else {
            PollChoice::First
        })
    }

    /// Votes for side 0 and side 1 of a poll.
    pub async fn poll_counts(&self, id: i64) -> anyhow::Result<(i64, i64)> {
        let (a, b): (i64, i64) = sqlx::query_as(
            "SELECT COUNT(*) FILTER (WHERE pick = 0), COUNT(*) FILTER (WHERE pick = 1)
             FROM party_votes WHERE game_id = $1 AND round = 0",
        )
        .bind(id)
        .fetch_one(self.pool())
        .await?;
        Ok((a, b))
    }

    /// Everyone who voted in a poll and the side they ended on.
    pub async fn poll_voters(&self, id: i64) -> anyhow::Result<Vec<(u64, i64)>> {
        let rows: Vec<(i64, i64)> =
            sqlx::query_as("SELECT voter_id, pick FROM party_votes WHERE game_id = $1 AND round = 0")
                .bind(id)
                .fetch_all(self.pool())
                .await?;
        Ok(rows.into_iter().map(|(v, p)| (v as u64, p)).collect())
    }
}
