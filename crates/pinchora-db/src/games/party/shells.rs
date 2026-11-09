use super::*;

/// What a player may do at the shell game table in a day.
#[derive(Debug, Clone, Copy)]
pub struct ShellLimits {
    pub plays: i32,
    pub loss: i64,
    pub win: i64,
}

#[derive(Debug, Clone)]
pub enum ShellStart {
    Started(GameSession),
    Broke {
        balance: i64,
    },
    /// Today's games are used up.
    TooMany,
    /// This stake would take the player past the day's loss limit; `net` is where they stand.
    Down {
        net: i64,
    },
    /// The player is up as far as the table lets them be today.
    Up {
        net: i64,
    },
}

#[derive(Debug, Clone)]
pub enum ShellPick {
    Done(ShellDone),
    NotYours,
    /// Over already, or out of time.
    Gone,
}

#[derive(Debug, Clone)]
pub struct ShellDone {
    pub session: GameSession,
    pub pearl: u8,
    pub picked: u8,
    /// Shells paid back; 0 for a wrong guess.
    pub paid: i64,
}

impl Db {
    // /Shells

    /// A player sits down at the shell game: the stake comes off their balance and the pearl is put
    /// under shell `pearl`. Refused when they cannot cover it or have used the day's allowance.
    pub async fn shell_start(&self, new: NewGame<'_>, limits: ShellLimits) -> anyhow::Result<ShellStart> {
        retry("shell_start", || self.shell_start_once(new, limits)).await
    }

    /// One attempt of [`Db::shell_start`].
    async fn shell_start_once(&self, new: NewGame<'_>, limits: ShellLimits) -> anyhow::Result<ShellStart> {
        let user = sql_id(new.host);
        let stake = new.bet;
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[user]).await?;
        sqlx::query("INSERT INTO users (id) VALUES ($1) ON CONFLICT (id) DO NOTHING")
            .bind(user)
            .execute(&mut *tx)
            .await?;
        let (balance,): (i64,) = sqlx::query_as("SELECT credits FROM users WHERE id = $1 FOR UPDATE")
            .bind(user)
            .fetch_one(&mut *tx)
            .await?;
        if balance < stake {
            return Ok(ShellStart::Broke { balance });
        }
        sqlx::query("INSERT INTO party_daily (user_id, day, game) VALUES ($1, $2, 'shellgame') ON CONFLICT DO NOTHING")
            .bind(user)
            .bind(today())
            .execute(&mut *tx)
            .await?;
        let (plays, net): (i32, i64) = sqlx::query_as(
            "SELECT plays, net FROM party_daily WHERE user_id = $1 AND day = $2 AND game = 'shellgame' FOR UPDATE",
        )
        .bind(user)
        .bind(today())
        .fetch_one(&mut *tx)
        .await?;
        if plays >= limits.plays {
            return Ok(ShellStart::TooMany);
        }
        if net - stake < -limits.loss {
            return Ok(ShellStart::Down { net });
        }
        if net >= limits.win {
            return Ok(ShellStart::Up { net });
        }
        sqlx::query("UPDATE users SET credits = credits - $2 WHERE id = $1")
            .bind(user)
            .bind(stake)
            .execute(&mut *tx)
            .await?;
        ledger(&mut tx, user, -stake, "shells_bet").await?;
        sqlx::query(
            "UPDATE party_daily SET plays = plays + 1, net = net - $4
             WHERE user_id = $1 AND day = $2 AND game = $3",
        )
        .bind(user)
        .bind(today())
        .bind("shellgame")
        .bind(stake)
        .execute(&mut *tx)
        .await?;
        let session: GameSession = sqlx::query_as(concat!(
            "INSERT INTO game_sessions (kind, guild_id, channel_id, host_id, guest_id, bet, status, state, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now() + make_interval(secs => $9))
             RETURNING ",
            columns!()
        ))
        .bind(new.kind)
        .bind(sql_id(new.guild_id))
        .bind(sql_id(new.channel_id))
        .bind(user)
        .bind(new.guest.map(sql_id))
        .bind(stake)
        .bind(new.status)
        .bind(new.state)
        .bind(new.ttl_secs as f64)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(ShellStart::Started(session))
    }

    /// The player picks a shell: the game ends, and a right guess is paid `payout(stake)`.
    pub async fn shell_pick(
        &self,
        session: i64,
        user: u64,
        pick: u8,
        payout: fn(i64) -> i64,
    ) -> anyhow::Result<ShellPick> {
        retry("shell_pick", || self.shell_pick_once(session, user, pick, payout)).await
    }

    /// One attempt of [`Db::shell_pick`]: the player's lock, then the table's row.
    async fn shell_pick_once(
        &self,
        session: i64,
        user: u64,
        pick: u8,
        payout: fn(i64) -> i64,
    ) -> anyhow::Result<ShellPick> {
        let mut tx = self.pool().begin().await?;
        lock_players(&mut tx, session).await?;
        let Some(game) = lock_game(&mut tx, session, "shellgame")
            .await?
            .filter(|s| s.status == ACTIVE)
        else {
            return Ok(ShellPick::Gone);
        };
        if game.host_id != sql_id(user) {
            return Ok(ShellPick::NotYours);
        }
        if game.expired() {
            return Ok(ShellPick::Gone);
        }
        // The state is the pearl's shell, then the table's language after a `|`.
        let pearl: u8 = game
            .state
            .split('|')
            .next()
            .unwrap_or_default()
            .parse()
            .map_err(|_| anyhow::anyhow!("bad shell game state {:?}", game.state))?;
        let paid = if pick == pearl { payout(game.bet) } else { 0 };
        if paid > 0 {
            credit(&mut tx, game.host_id, paid, "shells_win").await?;
            shell_net(&mut tx, session, game.host_id, paid, 0).await?;
        }
        finish(&mut tx, session, &format!("{pearl}:{pick}")).await?;
        tx.commit().await?;
        Ok(ShellPick::Done(ShellDone {
            session: game,
            pearl,
            picked: pick,
            paid,
        }))
    }

    /// Gives the stake back for a shell game nobody finished, once its time is up. `None` when it
    /// is over already or not late yet.
    pub async fn shell_refund(&self, session: i64) -> anyhow::Result<Option<GameSession>> {
        retry("shell_refund", || self.shell_refund_once(session)).await
    }

    /// One attempt of [`Db::shell_refund`].
    async fn shell_refund_once(&self, session: i64) -> anyhow::Result<Option<GameSession>> {
        let mut tx = self.pool().begin().await?;
        lock_players(&mut tx, session).await?;
        let Some(game) = lock_game(&mut tx, session, "shellgame")
            .await?
            .filter(|s| s.status == ACTIVE && s.expired())
        else {
            return Ok(None);
        };
        credit(&mut tx, game.host_id, game.bet, "shells_refund").await?;
        shell_net(&mut tx, session, game.host_id, game.bet, 1).await?;
        finish(&mut tx, session, "timeout").await?;
        tx.commit().await?;
        Ok(Some(game))
    }
}
