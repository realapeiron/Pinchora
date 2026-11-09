use super::*;

/// A server's storm surge settings.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PartySettings {
    pub guild_id: i64,
    pub surge_enabled: bool,
    pub surge_channel_id: Option<i64>,
    pub surge_every_hours: i32,
    pub surge_next_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub enum SurgeHelp {
    /// A sandbag was stacked: the helper's own count, the total stacked and how many have helped.
    Helped {
        session: GameSession,
        mine: i64,
        stacked: i64,
        helpers: i64,
    },
    /// The storm is over, or not running.
    Over,
    /// This helper has stacked all they may.
    Capped,
}

#[derive(Debug, Clone)]
pub struct SurgeSettled {
    pub session: GameSession,
    pub held: bool,
    /// Every helper with the sandbags they stacked and the shells they got.
    pub helpers: Vec<(PartyMember, i64)>,
}

impl Db {
    // /Surge

    /// A player stacks a sandbag on a running storm. Counts one sandbag for them, up to `max_helps`.
    pub async fn surge_help(&self, session: i64, user: u64, name: &str, max_helps: i64) -> anyhow::Result<SurgeHelp> {
        retry("surge_help", || self.surge_help_once(session, user, name, max_helps)).await
    }

    /// One attempt of [`Db::surge_help`].
    async fn surge_help_once(&self, session: i64, user: u64, name: &str, max_helps: i64) -> anyhow::Result<SurgeHelp> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        let Some(storm) = lock_game(&mut tx, session, "surge")
            .await?
            .filter(|s| s.status == ACTIVE && !s.expired())
        else {
            return Ok(SurgeHelp::Over);
        };
        let mine: Option<(i64,)> = sqlx::query_as(
            "INSERT INTO party_members (session_id, user_id, amount, name) VALUES ($1, $2, 1, $3)
             ON CONFLICT (session_id, user_id) DO UPDATE
                 SET amount = party_members.amount + 1, name = EXCLUDED.name
                 WHERE party_members.amount < $4
             RETURNING amount",
        )
        .bind(session)
        .bind(sql_id(user))
        .bind(name)
        .bind(max_helps)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((mine,)) = mine else {
            return Ok(SurgeHelp::Capped);
        };
        let (stacked, helpers) = totals(&mut tx, session).await?;
        tx.commit().await?;
        Ok(SurgeHelp::Helped {
            session: storm,
            mine,
            stacked,
            helpers,
        })
    }

    /// Storms that are running and not out of time, oldest first, for keeping their card fresh.
    pub async fn running_surges(&self, limit: i64) -> anyhow::Result<Vec<GameSession>> {
        Ok(sqlx::query_as::<_, GameSession>(concat!(
            "SELECT ",
            columns!(),
            " FROM game_sessions
             WHERE kind = 'surge' AND status = 'active' AND expires_at > now() ORDER BY id LIMIT $1"
        ))
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }

    /// Sandbags stacked and how many have helped so far.
    pub async fn surge_totals(&self, session: i64) -> anyhow::Result<(i64, i64)> {
        let mut tx = self.pool().begin().await?;
        let counts = totals(&mut tx, session).await?;
        tx.commit().await?;
        Ok(counts)
    }

    /// Ends a storm: marks it done and pays every helper, in one transaction. `held` says whether the
    /// wall held; a storm that did not hold is only ended once its time is up (`only_if_expired`).
    /// `reward` is what a helper gets for the sandbags they stacked, paid up to `daily_cap` a day.
    pub async fn surge_settle(
        &self,
        session: i64,
        held: bool,
        only_if_expired: bool,
        daily_cap: i64,
        reward: fn(bool, i64) -> i64,
    ) -> anyhow::Result<Option<SurgeSettled>> {
        retry("surge_settle", || {
            self.surge_settle_once(session, held, only_if_expired, daily_cap, reward)
        })
        .await
    }

    /// One attempt of [`Db::surge_settle`]: every helper's lock, then the storm's row.
    async fn surge_settle_once(
        &self,
        session: i64,
        held: bool,
        only_if_expired: bool,
        daily_cap: i64,
        reward: fn(bool, i64) -> i64,
    ) -> anyhow::Result<Option<SurgeSettled>> {
        let mut tx = self.pool().begin().await?;
        lock_members(&mut tx, session).await?;
        let Some(storm) = lock_game(&mut tx, session, "surge")
            .await?
            .filter(|s| s.status == ACTIVE && (!only_if_expired || s.expired()))
        else {
            return Ok(None);
        };
        let helpers = members_locked(&mut tx, session).await?;
        let mut order: Vec<&PartyMember> = helpers.iter().collect();
        order.sort_by_key(|m| m.user_id);
        let mut paid = Vec::with_capacity(helpers.len());
        for helper in order {
            let want = reward(held, helper.amount);
            let got = if want > 0 {
                grant_capped(&mut tx, helper.user_id, "surge", want, daily_cap, "surge_reward").await?
            } else {
                0
            };
            paid.push((helper.user_id, got));
        }
        finish(&mut tx, session, if held { "held" } else { "broken" }).await?;
        tx.commit().await?;
        let helpers = helpers
            .into_iter()
            .map(|h| {
                let got = paid
                    .iter()
                    .find(|(user, _)| *user == h.user_id)
                    .map_or(0, |(_, got)| *got);
                (h, got)
            })
            .collect();
        Ok(Some(SurgeSettled {
            session: storm,
            held,
            helpers,
        }))
    }
}
