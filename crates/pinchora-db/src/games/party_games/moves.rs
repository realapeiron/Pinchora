use super::*;

impl Db {
    /// Moves a game on, if it is still at `expect_seq` and not over: sets its phase, round, state and
    /// deadline, applies the player changes, files the entries and pays what a finished game pays, all
    /// in one transaction. `None` when somebody moved first; nothing has changed then, and the caller
    /// has nothing to announce.
    pub async fn commit_party(
        &self,
        id: i64,
        expect_seq: i32,
        commit: Commit<'_>,
    ) -> anyhow::Result<Option<Committed>> {
        retry("commit_party", || self.commit_party_once(id, expect_seq, commit)).await
    }

    /// One attempt of [`Db::commit_party`]: the locks of every player the move touches (the changes, the
    /// entries and the ones paid are all in its arguments), then the game's row.
    async fn commit_party_once(
        &self,
        id: i64,
        expect_seq: i32,
        commit: Commit<'_>,
    ) -> anyhow::Result<Option<Committed>> {
        let mut tx = self.pool().begin().await?;
        let touched: Vec<i64> = commit
            .players
            .iter()
            .map(|change| change.user)
            .chain(commit.entries.iter().map(|entry| entry.user))
            .chain(
                commit
                    .pay
                    .iter()
                    .flat_map(|pay| pay.shells.iter().map(|(user, _)| *user)),
            )
            .map(sql_id)
            .collect();
        lock_users(&mut tx, &touched).await?;
        let moved: Option<PartyGame> = sqlx::query_as(concat!(
            "UPDATE party_games
             SET phase = $3, round = $4, state = $5, deadline = $6, status = $7, seq = seq + 1
             WHERE id = $1 AND seq = $2 AND status <> 'done'
             RETURNING ",
            game_columns!()
        ))
        .bind(id)
        .bind(expect_seq)
        .bind(commit.phase)
        .bind(commit.round)
        .bind(commit.state)
        .bind(commit.deadline)
        .bind(commit.status)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(game) = moved else {
            return Ok(None);
        };
        for change in commit.players {
            sqlx::query(
                "UPDATE party_players
                 SET seat = COALESCE($3, seat), lives = COALESCE($4, lives), score = score + $5,
                     eliminated = COALESCE($6, eliminated)
                 WHERE game_id = $1 AND user_id = $2",
            )
            .bind(id)
            .bind(sql_id(change.user))
            .bind(change.seat)
            .bind(change.lives)
            .bind(change.score)
            .bind(change.eliminated)
            .execute(&mut *tx)
            .await?;
        }
        for entry in commit.entries {
            sqlx::query(
                "INSERT INTO party_entries (game_id, round, user_id, text) VALUES ($1, $2, $3, $4)
                 ON CONFLICT (game_id, round, user_id) DO UPDATE SET text = EXCLUDED.text",
            )
            .bind(id)
            .bind(entry.round)
            .bind(sql_id(entry.user))
            .bind(entry.text)
            .execute(&mut *tx)
            .await?;
        }
        let mut paid = Vec::new();
        if let Some(pay) = commit.pay {
            // In user order, so two games ending together cannot wait on each other's rows.
            let mut shells = pay.shells.to_vec();
            shells.sort_unstable();
            for (user, amount) in shells.into_iter().filter(|(_, amount)| *amount > 0) {
                let (plays,): (i32,) = sqlx::query_as(
                    "INSERT INTO party_daily (user_id, day, game, plays) VALUES ($1, $2, $3, 1)
                     ON CONFLICT (user_id, day, game) DO UPDATE SET plays = party_daily.plays + 1
                     RETURNING plays",
                )
                .bind(sql_id(user))
                .bind(pay.day)
                .bind(pay.game)
                .fetch_one(&mut *tx)
                .await?;
                if plays <= pay.cap {
                    earn(&mut tx, user, amount, pay.reason).await?;
                    paid.push((user, amount));
                } else {
                    paid.push((user, 0));
                }
            }
        }
        tx.commit().await?;
        Ok(Some(Committed { game, paid }))
    }
}
