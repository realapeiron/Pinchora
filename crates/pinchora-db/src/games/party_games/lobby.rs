use super::*;

impl Db {
    /// Opens a game with its host as the first player.
    pub async fn create_party(&self, new: NewParty<'_>) -> anyhow::Result<PartyGame> {
        retry("create_party", || self.create_party_once(new)).await
    }

    /// One attempt of [`Db::create_party`].
    async fn create_party_once(&self, new: NewParty<'_>) -> anyhow::Result<PartyGame> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(new.host)]).await?;
        crate::games::reef_party::guard_channel(&mut tx, new.channel_id).await?;
        let game: PartyGame = sqlx::query_as(concat!(
            "INSERT INTO party_games (kind, guild_id, channel_id, host_id, status, phase, rounds, state, deadline)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING ",
            game_columns!()
        ))
        .bind(new.kind)
        .bind(sql_id(new.guild_id))
        .bind(sql_id(new.channel_id))
        .bind(sql_id(new.host))
        .bind(new.status)
        .bind(new.phase)
        .bind(new.rounds)
        .bind(new.state)
        .bind(new.deadline)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO party_players (game_id, user_id, name, seat) VALUES ($1, $2, $3, 0)")
            .bind(game.id)
            .bind(sql_id(new.host))
            .bind(new.host_name)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(game)
    }

    pub async fn party(&self, id: i64) -> anyhow::Result<Option<PartyGame>> {
        Ok(
            sqlx::query_as::<_, PartyGame>(concat!("SELECT ", game_columns!(), " FROM party_games WHERE id = $1"))
                .bind(id)
                .fetch_optional(self.pool())
                .await?,
        )
    }

    /// The players of a game in seat order.
    pub async fn party_players(&self, id: i64) -> anyhow::Result<Vec<PartyPlayer>> {
        Ok(sqlx::query_as::<_, PartyPlayer>(concat!(
            "SELECT ",
            player_columns!(),
            " FROM party_players WHERE game_id = $1 ORDER BY seat, user_id"
        ))
        .bind(id)
        .fetch_all(self.pool())
        .await?)
    }

    /// A lobby or game still going in `channel`, which keeps another from starting there. Polls do not
    /// count, and neither does a game more than five minutes past its deadline: the ticker will close it.
    pub async fn party_in_channel(&self, channel: u64, now: DateTime<Utc>) -> anyhow::Result<Option<PartyGame>> {
        Ok(sqlx::query_as::<_, PartyGame>(concat!(
            "SELECT ",
            game_columns!(),
            " FROM party_games
             WHERE channel_id = $1 AND status <> 'done' AND kind <> 'wouldyourrather'
               AND deadline > $2 - interval '5 minutes'
             ORDER BY id DESC LIMIT 1"
        ))
        .bind(sql_id(channel))
        .bind(now)
        .fetch_optional(self.pool())
        .await?)
    }

    /// Remembers which message shows the game, so the ticker can edit it.
    pub async fn set_party_message(&self, id: i64, channel: u64, message: u64) -> anyhow::Result<()> {
        sqlx::query("UPDATE party_games SET channel_id = $2, message_id = $3 WHERE id = $1 AND status <> 'done'")
            .bind(id)
            .bind(sql_id(channel))
            .bind(sql_id(message))
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Adds a player to a lobby that has room.
    pub async fn join_party(
        &self,
        id: i64,
        user: u64,
        name: &str,
        max: usize,
        now: DateTime<Utc>,
    ) -> anyhow::Result<PartyJoin> {
        retry("join_party", || self.join_party_once(id, user, name, max, now)).await
    }

    /// One attempt of [`Db::join_party`].
    async fn join_party_once(
        &self,
        id: i64,
        user: u64,
        name: &str,
        max: usize,
        now: DateTime<Utc>,
    ) -> anyhow::Result<PartyJoin> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        let Some(game) = lock_game(&mut tx, id).await? else {
            return Ok(PartyJoin::Closed);
        };
        if !game.in_lobby() || game.late(now) {
            return Ok(PartyJoin::Closed);
        }
        let (seats, next, me): (i64, i32, i64) = sqlx::query_as(
            "SELECT COUNT(*), COALESCE(MAX(seat) + 1, 0)::INTEGER,
                    COUNT(*) FILTER (WHERE user_id = $2)
             FROM party_players WHERE game_id = $1",
        )
        .bind(id)
        .bind(sql_id(user))
        .fetch_one(&mut *tx)
        .await?;
        if me > 0 {
            return Ok(PartyJoin::Already);
        }
        if seats as usize >= max {
            return Ok(PartyJoin::Full);
        }
        sqlx::query("INSERT INTO party_players (game_id, user_id, name, seat) VALUES ($1, $2, $3, $4)")
            .bind(id)
            .bind(sql_id(user))
            .bind(name)
            .bind(next)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE party_games SET seq = seq + 1 WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(PartyJoin::Joined)
    }

    /// Takes a player out of a lobby.
    pub async fn leave_party(&self, id: i64, user: u64, now: DateTime<Utc>) -> anyhow::Result<PartyLeave> {
        retry("leave_party", || self.leave_party_once(id, user, now)).await
    }

    /// One attempt of [`Db::leave_party`].
    async fn leave_party_once(&self, id: i64, user: u64, now: DateTime<Utc>) -> anyhow::Result<PartyLeave> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        let Some(game) = lock_game(&mut tx, id).await? else {
            return Ok(PartyLeave::Closed);
        };
        if !game.in_lobby() || game.late(now) {
            return Ok(PartyLeave::Closed);
        }
        if game.host_id == sql_id(user) {
            return Ok(PartyLeave::Host);
        }
        let gone = sqlx::query("DELETE FROM party_players WHERE game_id = $1 AND user_id = $2")
            .bind(id)
            .bind(sql_id(user))
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if gone == 0 {
            return Ok(PartyLeave::NotIn);
        }
        sqlx::query("UPDATE party_games SET seq = seq + 1 WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(PartyLeave::Left)
    }
}
