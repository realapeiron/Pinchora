//! D9 uses existing sessions, members, stats and daily economy tables.
//! Identity stays in registered columns; JSON state refers only to numbered seats.
use crate::{
    Db, GameSession, NewGame,
    economy::ledger,
    locks::{lock_users, retry},
    sql_id,
};
use chrono::{DateTime, Utc};

macro_rules! columns {
    () => {
        "id, kind, guild_id, channel_id, message_id, host_id, guest_id, bet, status, state, expires_at"
    };
}

pub struct ReefMove<'a> {
    pub state: &'a str,
    pub status: &'a str,
    pub deadline: DateTime<Utc>,
    pub now: DateTime<Utc>,
    pub timeout: bool,
    pub join: Option<(u64, &'a str, i16)>,
    /// Seat, score, winner, eligible for 15 shells. Empty unless the game genuinely finished.
    pub results: &'a [(i16, i32, bool, bool)],
}

impl Db {
    pub async fn reef_open(
        &self,
        new: NewGame<'_>,
        name: &str,
        now: DateTime<Utc>,
    ) -> anyhow::Result<Option<GameSession>> {
        retry("reef_open", || self.reef_open_once(new, name, now)).await
    }

    async fn reef_open_once(
        &self,
        new: NewGame<'_>,
        name: &str,
        now: DateTime<Utc>,
    ) -> anyhow::Result<Option<GameSession>> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(new.host)]).await?;
        channel_lock(&mut tx, new.channel_id).await?;
        let (busy,): (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM game_sessions WHERE channel_id=$1 AND status<>'done')
                 OR EXISTS(SELECT 1 FROM party_games WHERE channel_id=$1 AND status<>'done')",
        )
        .bind(sql_id(new.channel_id))
        .fetch_one(&mut *tx)
        .await?;
        if busy {
            return Ok(None);
        }
        let game = sqlx::query_as::<_, GameSession>(concat!(
            "INSERT INTO game_sessions(kind,guild_id,channel_id,host_id,status,state,expires_at)
             VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING ",
            columns!()
        ))
        .bind(new.kind)
        .bind(sql_id(new.guild_id))
        .bind(sql_id(new.channel_id))
        .bind(sql_id(new.host))
        .bind(new.status)
        .bind(new.state)
        .bind(now + chrono::Duration::seconds(new.ttl_secs))
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO party_members(session_id,user_id,choice,name) VALUES($1,$2,0,$3)")
            .bind(game.id)
            .bind(sql_id(new.host))
            .bind(name)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Some(game))
    }

    /// CAS state, membership, statistics and capped payout in one transaction.
    pub async fn reef_move(&self, old: &GameSession, m: ReefMove<'_>) -> anyhow::Result<Option<GameSession>> {
        retry("reef_move", || self.reef_move_once(old, &m)).await
    }

    async fn reef_move_once(&self, old: &GameSession, m: &ReefMove<'_>) -> anyhow::Result<Option<GameSession>> {
        let mut tx = self.pool().begin().await?;
        let members: Vec<(i64, i16)> =
            sqlx::query_as("SELECT user_id,choice FROM party_members WHERE session_id=$1 ORDER BY user_id")
                .bind(old.id)
                .fetch_all(&mut *tx)
                .await?;
        let mut ids: Vec<i64> = members.iter().map(|(id, _)| *id).collect();
        ids.push(old.host_id);
        if let Some((id, _, _)) = m.join {
            ids.push(sql_id(id));
        }
        lock_users(&mut tx, &ids).await?;
        let moved = sqlx::query_as::<_, GameSession>(concat!(
            "UPDATE game_sessions SET state=$3,status=$4,expires_at=$5
             WHERE id=$1 AND state=$2 AND status<>'done' AND kind LIKE 'd9:%'
             AND (($6 AND expires_at<=$7) OR (NOT $6 AND expires_at>$7)) RETURNING ",
            columns!()
        ))
        .bind(old.id)
        .bind(&old.state)
        .bind(m.state)
        .bind(m.status)
        .bind(m.deadline)
        .bind(m.timeout)
        .bind(m.now)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(game) = moved else {
            return Ok(None);
        };
        if let Some((id, name, seat)) = m.join {
            sqlx::query("INSERT INTO party_members(session_id,user_id,choice,name) VALUES($1,$2,$3,$4)")
                .bind(old.id)
                .bind(sql_id(id))
                .bind(seat)
                .bind(name)
                .execute(&mut *tx)
                .await?;
        }
        if m.status == "done" {
            // Re-read after user locks: erasure may have removed a member while we waited.
            // A concurrent join changes the CAS state, so it cannot introduce unlocked members here.
            let members: Vec<(i64, i16)> =
                sqlx::query_as("SELECT user_id,choice FROM party_members WHERE session_id=$1 ORDER BY user_id")
                    .bind(old.id)
                    .fetch_all(&mut *tx)
                    .await?;
            for (user, seat) in members {
                let Some((_, score, won, reward)) = m.results.iter().find(|(s, _, _, _)| *s == seat) else {
                    continue;
                };
                sqlx::query(
                    "INSERT INTO game_stats(user_id,game,played,won,best) VALUES($1,$2,1,$3,$4)
                     ON CONFLICT(user_id,game) DO UPDATE SET played=game_stats.played+1,
                     won=game_stats.won+EXCLUDED.won,best=GREATEST(game_stats.best,EXCLUDED.best)",
                )
                .bind(user)
                .bind(game.kind.trim_start_matches("d9:"))
                .bind(i32::from(*won))
                .bind(score)
                .execute(&mut *tx)
                .await?;
                if *reward {
                    reward_winner(&mut tx, user, m.now).await?;
                }
            }
        }
        tx.commit().await?;
        Ok(Some(game))
    }

    pub async fn reef_due(&self, now: DateTime<Utc>) -> anyhow::Result<Vec<GameSession>> {
        Ok(sqlx::query_as::<_, GameSession>(concat!(
            "SELECT ",
            columns!(),
            " FROM game_sessions WHERE kind LIKE 'd9:%' AND status<>'done'
             AND expires_at<=$1 ORDER BY expires_at LIMIT 50"
        ))
        .bind(now)
        .fetch_all(self.pool())
        .await?)
    }
}

async fn reward_winner(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user: i64,
    now: DateTime<Utc>,
) -> anyhow::Result<()> {
    let day = now.timestamp().div_euclid(86400) as i32;
    let paid: Option<(i64,)> = sqlx::query_as(
        "INSERT INTO party_daily(user_id,day,game,paid) VALUES($1,$2,'party_v2',15)
         ON CONFLICT(user_id,day,game) DO UPDATE SET paid=party_daily.paid+15
         WHERE party_daily.paid+15<=90 RETURNING paid",
    )
    .bind(user)
    .bind(day)
    .fetch_optional(&mut **tx)
    .await?;
    if paid.is_some() {
        sqlx::query(
            "INSERT INTO users(id,credits) VALUES($1,15) ON CONFLICT(id) DO UPDATE SET credits=users.credits+15",
        )
        .bind(user)
        .execute(&mut **tx)
        .await?;
        ledger(tx, user, 15, "party_v2_win").await?;
    }
    Ok(())
}

async fn channel_lock(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, channel: u64) -> anyhow::Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('reef-game-channel:' || $1::bigint::text, 0))")
        .bind(sql_id(channel))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Old game creators share D9's channel lock, so a concurrent legacy start cannot overlap it.
pub(crate) async fn guard_channel(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, channel: u64) -> anyhow::Result<()> {
    channel_lock(tx, channel).await?;
    let (busy,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM game_sessions WHERE channel_id=$1 AND kind LIKE 'd9:%' AND status<>'done')",
    )
    .bind(sql_id(channel))
    .fetch_one(&mut **tx)
    .await?;
    anyhow::ensure!(!busy, "a reef game is already running in this channel");
    Ok(())
}
