use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DerbyJoin {
    Joined,
    /// The player was in already and picked another crab; nothing was charged again.
    Moved,
    /// The lobby is closed, or gone.
    Closed,
    Broke {
        balance: i64,
    },
    Full,
    /// This stake would take the player past the day's derby loss limit; `net` is where they stand
    /// (shells won less shells staked and not given back).
    Down {
        net: i64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DerbyLeave {
    /// The player left; `refund` is the stake they got back.
    Left {
        refund: i64,
    },
    NotIn,
    Closed,
}

/// What a derby came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DerbyOutcome {
    /// Nobody (or too few for a race with shells at stake) joined; every stake is back.
    Cancelled,
    /// Nobody backed the winner; every stake is back.
    Refunded,
    /// Those who backed the winner shared the pool: each as (player, shells paid).
    Paid(Vec<(u64, i64)>),
    /// A race for fun: nothing was staked. The players who backed the winner.
    Friendly(Vec<u64>),
}

#[derive(Debug, Clone)]
pub struct DerbyResult {
    /// The lobby as it was before the race.
    pub session: GameSession,
    pub racers: Vec<PartyMember>,
    /// The winning crab.
    pub winner: i16,
    pub outcome: DerbyOutcome,
}

impl Db {
    // /Derby

    /// A player picks a crab in an open derby. With a stake the stake comes off their balance, once, and
    /// counts against `loss_limit` shells a day; picking again moves them to the other crab for free.
    pub async fn derby_join(
        &self,
        session: i64,
        user: u64,
        name: &str,
        crab: i16,
        max_racers: i64,
        loss_limit: i64,
    ) -> anyhow::Result<DerbyJoin> {
        retry("derby_join", || {
            self.derby_join_once(session, user, name, crab, max_racers, loss_limit)
        })
        .await
    }

    /// One attempt of [`Db::derby_join`].
    async fn derby_join_once(
        &self,
        session: i64,
        user: u64,
        name: &str,
        crab: i16,
        max_racers: i64,
        loss_limit: i64,
    ) -> anyhow::Result<DerbyJoin> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        let Some(lobby) = lock_open(&mut tx, session, "derby").await? else {
            return Ok(DerbyJoin::Closed);
        };
        let user = sql_id(user);
        let moved =
            sqlx::query("UPDATE party_members SET choice = $3, name = $4 WHERE session_id = $1 AND user_id = $2")
                .bind(session)
                .bind(user)
                .bind(crab)
                .bind(name)
                .execute(&mut *tx)
                .await?
                .rows_affected();
        if moved == 1 {
            tx.commit().await?;
            return Ok(DerbyJoin::Moved);
        }
        let (racers,): (i64,) = sqlx::query_as("SELECT count(*) FROM party_members WHERE session_id = $1")
            .bind(session)
            .fetch_one(&mut *tx)
            .await?;
        if racers >= max_racers {
            return Ok(DerbyJoin::Full);
        }
        if lobby.bet > 0 {
            sqlx::query("INSERT INTO users (id) VALUES ($1) ON CONFLICT (id) DO NOTHING")
                .bind(user)
                .execute(&mut *tx)
                .await?;
            let (balance,): (i64,) = sqlx::query_as("SELECT credits FROM users WHERE id = $1 FOR UPDATE")
                .bind(user)
                .fetch_one(&mut *tx)
                .await?;
            if balance < lobby.bet {
                return Ok(DerbyJoin::Broke { balance });
            }
            // The day's row is the one of the transaction's own clock, which stamps `joined_at` too: that
            // is the row a refund or a payout finds again later.
            sqlx::query(concat!(
                "INSERT INTO party_daily (user_id, day, game) VALUES ($1, ",
                day_of!("now()"),
                ", 'derby') ON CONFLICT DO NOTHING"
            ))
            .bind(user)
            .execute(&mut *tx)
            .await?;
            let (net,): (i64,) = sqlx::query_as(concat!(
                "SELECT net FROM party_daily WHERE user_id = $1 AND day = ",
                day_of!("now()"),
                " AND game = 'derby' FOR UPDATE"
            ))
            .bind(user)
            .fetch_one(&mut *tx)
            .await?;
            if net - lobby.bet < -loss_limit {
                return Ok(DerbyJoin::Down { net });
            }
            sqlx::query("UPDATE users SET credits = credits - $2 WHERE id = $1")
                .bind(user)
                .bind(lobby.bet)
                .execute(&mut *tx)
                .await?;
            ledger(&mut tx, user, -lobby.bet, "derby_bet").await?;
            sqlx::query(concat!(
                "UPDATE party_daily SET net = net - $2 WHERE user_id = $1 AND day = ",
                day_of!("now()"),
                " AND game = 'derby'"
            ))
            .bind(user)
            .bind(lobby.bet)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "INSERT INTO party_members (session_id, user_id, choice, amount, name) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(session)
        .bind(user)
        .bind(crab)
        .bind(lobby.bet)
        .bind(name)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(DerbyJoin::Joined)
    }

    /// A racer leaves an open derby and gets their stake back.
    pub async fn derby_leave(&self, session: i64, user: u64) -> anyhow::Result<DerbyLeave> {
        retry("derby_leave", || self.derby_leave_once(session, user)).await
    }

    /// One attempt of [`Db::derby_leave`].
    async fn derby_leave_once(&self, session: i64, user: u64) -> anyhow::Result<DerbyLeave> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        if lock_open(&mut tx, session, "derby").await?.is_none() {
            return Ok(DerbyLeave::Closed);
        }
        let user = sql_id(user);
        let held: Option<(i64,)> =
            sqlx::query_as("SELECT amount FROM party_members WHERE session_id = $1 AND user_id = $2 FOR UPDATE")
                .bind(session)
                .bind(user)
                .fetch_optional(&mut *tx)
                .await?;
        let Some((refund,)) = held else {
            return Ok(DerbyLeave::NotIn);
        };
        if refund > 0 {
            credit(&mut tx, user, refund, "derby_refund").await?;
            derby_net(&mut tx, session, user, refund).await?;
        }
        sqlx::query("DELETE FROM party_members WHERE session_id = $1 AND user_id = $2")
            .bind(session)
            .bind(user)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(DerbyLeave::Left { refund })
    }

    /// Runs the derby: ends the lobby and pays, in one transaction. `winner` is the crab that won the
    /// race the lobby's seed says. `forced_by` is the host starting the race early; without it the
    /// lobby must have run out of time. `None` when somebody else ended it first.
    pub async fn derby_settle(
        &self,
        session: i64,
        forced_by: Option<u64>,
        min_staked: usize,
        winner: i16,
    ) -> anyhow::Result<Option<DerbyResult>> {
        retry("derby_settle", || {
            self.derby_settle_once(session, forced_by, min_staked, winner)
        })
        .await
    }

    /// One attempt of [`Db::derby_settle`]: every racer's lock, then the lobby's row.
    async fn derby_settle_once(
        &self,
        session: i64,
        forced_by: Option<u64>,
        min_staked: usize,
        winner: i16,
    ) -> anyhow::Result<Option<DerbyResult>> {
        let mut tx = self.pool().begin().await?;
        lock_members(&mut tx, session).await?;
        let Some(lobby) = lock_game(&mut tx, session, "derby").await? else {
            return Ok(None);
        };
        let due = match forced_by {
            Some(host) => lobby.host_id == sql_id(host),
            None => lobby.expired(),
        };
        if lobby.status != PENDING || !due {
            return Ok(None);
        }
        let racers = members_locked(&mut tx, session).await?;
        let staked = lobby.bet > 0;
        let outcome = if racers.is_empty() || (staked && racers.len() < min_staked) {
            refund_all(&mut tx, session, &racers).await?;
            DerbyOutcome::Cancelled
        } else if !staked {
            let winners = racers.iter().filter(|r| r.choice == winner).map(|r| r.user_id as u64);
            DerbyOutcome::Friendly(winners.collect())
        } else {
            let winners: Vec<&PartyMember> = racers.iter().filter(|r| r.choice == winner).collect();
            if winners.is_empty() {
                refund_all(&mut tx, session, &racers).await?;
                DerbyOutcome::Refunded
            } else {
                let pool: i64 = racers.iter().map(|r| r.amount).sum();
                let shares = split(pool, winners.len());
                let mut paid: Vec<(u64, i64)> =
                    winners.iter().zip(shares).map(|(w, s)| (w.user_id as u64, s)).collect();
                // Pay in id order, like every multi-player payout, so two settles cannot deadlock.
                paid.sort_unstable();
                for (user, share) in &paid {
                    credit(&mut tx, sql_id(*user), *share, "derby_win").await?;
                    derby_net(&mut tx, session, sql_id(*user), *share).await?;
                }
                paid.sort_by_key(|(user, _)| racers.iter().position(|r| r.user_id as u64 == *user));
                DerbyOutcome::Paid(paid)
            }
        };
        finish(&mut tx, session, "done").await?;
        tx.commit().await?;
        Ok(Some(DerbyResult {
            session: lobby,
            racers,
            winner,
            outcome,
        }))
    }
}
