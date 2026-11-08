//! Durable room sessions. Rules are supplied by the game adapter, under the session lock.
use crate::{
    Db, ReefFence,
    game_rewards::{capped_win, game_stat},
    locks::{lock_users, try_lock_users},
    sql_id,
};
use chrono::{DateTime, Utc};
use serde_json::Value;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReefGame {
    pub id: i64,
    pub room_key: String,
    pub owner_id: i64,
    pub host_id: i64,
    pub kind: String,
    pub placement_id: Option<i64>,
    pub layout_revision: Option<i64>,
    pub state: Value,
    pub revision: i64,
    pub finished: bool,
    pub deadline: DateTime<Utc>,
}
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReefGamePlayer {
    pub user_id: i64,
    pub side: i16,
    pub answer: Option<i16>,
    pub paid: i64,
}
pub struct NewReefGame<'a> {
    pub room: &'a str,
    pub owner: u64,
    pub host: u64,
    pub kind: &'a str,
    pub request: &'a str,
    pub placement: Option<i64>,
    pub layout_revision: Option<i64>,
    pub state: Value,
}
/// A validated transition; payout and winner come exclusively from server rules.
pub struct ReefGameStep {
    pub state: Value,
    pub finished: bool,
    pub seconds: i64,
    pub join: bool,
    pub answer: Option<i16>,
    pub winners: Vec<i16>,
    pub reward: i64,
    pub cap: i64,
}
impl Db {
    pub async fn reef_late_games(&self) -> anyhow::Result<Vec<ReefGame>> {
        Ok(sqlx::query_as("SELECT id,room_key,owner_id,host_id,kind,placement_id,layout_revision,state,revision,finished,deadline FROM reef_games WHERE NOT finished AND deadline<=now() ORDER BY deadline LIMIT 100").fetch_all(self.pool()).await?)
    }
    pub async fn reef_prune_games(&self) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM reef_games WHERE id IN (SELECT id FROM reef_games WHERE finished AND created_at<now()-interval '7 days' ORDER BY id LIMIT 500 FOR UPDATE SKIP LOCKED)").execute(self.pool()).await?;
        sqlx::query("DELETE FROM game_reward_daily WHERE day<floor(extract(epoch FROM now())/86400)::int-2")
            .execute(self.pool())
            .await?;
        Ok(())
    }
    pub async fn reef_game(&self, id: i64) -> anyhow::Result<Option<ReefGame>> {
        Ok(sqlx::query_as("SELECT id,room_key,owner_id,host_id,kind,placement_id,layout_revision,state,revision,finished,deadline FROM reef_games WHERE id=$1").bind(id).fetch_optional(self.pool()).await?)
    }
    pub async fn reef_game_players(&self, id: i64) -> anyhow::Result<Vec<ReefGamePlayer>> {
        Ok(
            sqlx::query_as("SELECT user_id,side,answer,paid FROM reef_game_players WHERE game_id=$1 ORDER BY side")
                .bind(id)
                .fetch_all(self.pool())
                .await?,
        )
    }
    pub async fn reef_room_games(&self, room: &str) -> anyhow::Result<Vec<ReefGame>> {
        Ok(sqlx::query_as("SELECT id,room_key,owner_id,host_id,kind,placement_id,layout_revision,state,revision,finished,deadline FROM reef_games WHERE room_key=$1 AND created_at>now()-interval '2 days' ORDER BY id DESC LIMIT 32").bind(room).fetch_all(self.pool()).await?)
    }
    pub async fn reef_create_game(&self, new: NewReefGame<'_>) -> anyhow::Result<ReefGame> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(new.host), sql_id(new.owner)]).await?;
        if let Some(existing)=sqlx::query_as::<_,ReefGame>("SELECT id,room_key,owner_id,host_id,kind,placement_id,layout_revision,state,revision,finished,deadline FROM reef_games WHERE host_id=$1 AND request_id=$2")
            .bind(sql_id(new.host)).bind(new.request).fetch_optional(&mut *tx).await? {
            anyhow::ensure!(existing.room_key==new.room && existing.kind==new.kind && existing.placement_id==new.placement,"request_conflict");
            return Ok(existing);
        }
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM rooms WHERE user_id=$1 AND idx=$2)")
            .bind(sql_id(new.owner))
            .bind(new.room.split(':').nth(2).and_then(|s| s.parse::<i16>().ok()))
            .fetch_one(&mut *tx)
            .await?;
        anyhow::ensure!(exists, "invalid_room");
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM reef_games WHERE room_key=$1 AND NOT finished AND deadline>now()")
                .bind(new.room)
                .fetch_one(&mut *tx)
                .await?;
        anyhow::ensure!(count < 8, "room_busy");
        if let Some(placement) = new.placement {
            let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM placements p JOIN houses h ON h.user_id=p.user_id WHERE p.id=$1 AND p.user_id=$2 AND p.room_idx=$3 AND h.edit_revision=$4)")
                .bind(placement).bind(sql_id(new.owner)).bind(new.room.split(':').nth(2).and_then(|s|s.parse::<i16>().ok())).bind(new.layout_revision).fetch_one(&mut *tx).await?;
            anyhow::ensure!(valid, "stale_layout");
            let occupied: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM reef_games WHERE room_key=$1 AND placement_id=$2 AND NOT finished)",
            )
            .bind(new.room)
            .bind(placement)
            .fetch_one(&mut *tx)
            .await?;
            anyhow::ensure!(!occupied, "room_busy");
        }
        let game=sqlx::query_as::<_,ReefGame>("INSERT INTO reef_games(room_key,owner_id,host_id,kind,request_id,placement_id,layout_revision,state,deadline) VALUES($1,$2,$3,$4,$5,$6,$7,$8,now()+interval '5 minutes') RETURNING id,room_key,owner_id,host_id,kind,placement_id,layout_revision,state,revision,finished,deadline")
            .bind(new.room).bind(sql_id(new.owner)).bind(sql_id(new.host)).bind(new.kind).bind(new.request).bind(new.placement).bind(new.layout_revision).bind(new.state).fetch_one(&mut *tx).await?;
        sqlx::query("INSERT INTO reef_game_players(game_id,user_id,side) VALUES($1,$2,0)")
            .bind(game.id)
            .bind(sql_id(new.host))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(game)
    }
    /// State, stats, cap and ledger commit together. Retrying a committed request only reads it.
    pub async fn reef_game_step<F>(
        &self,
        id: i64,
        user: u64,
        request: &str,
        payload: &Value,
        fence: Option<&ReefFence>,
        step: F,
    ) -> anyhow::Result<ReefGame>
    where
        F: Fn(&ReefGame, &[ReefGamePlayer], i64) -> anyhow::Result<ReefGameStep>,
    {
        crate::locks::retry("reef_game_step", || {
            self.reef_game_step_once(id, user, request, payload, fence, &step)
        })
        .await
    }
    async fn reef_game_step_once<F>(
        &self,
        id: i64,
        user: u64,
        request: &str,
        payload: &Value,
        fence: Option<&ReefFence>,
        step: F,
    ) -> anyhow::Result<ReefGame>
    where
        F: Fn(&ReefGame, &[ReefGamePlayer], i64) -> anyhow::Result<ReefGameStep>,
    {
        let mut tx = self.pool().begin().await?;
        let mut users: Vec<i64> = sqlx::query_scalar("SELECT user_id FROM reef_game_players WHERE game_id=$1")
            .bind(id)
            .fetch_all(&mut *tx)
            .await?;
        users.push(sql_id(user));
        lock_users(&mut tx, &users).await?;
        if let Some(fence) = fence {
            anyhow::ensure!(fence.lock_valid(&mut tx).await?, "stale_epoch");
        }
        let game:ReefGame=sqlx::query_as("SELECT id,room_key,owner_id,host_id,kind,placement_id,layout_revision,state,revision,finished,deadline FROM reef_games WHERE id=$1 FOR UPDATE").bind(id).fetch_optional(&mut *tx).await?.ok_or_else(||anyhow::anyhow!("game_missing"))?;
        let players: Vec<ReefGamePlayer> =
            sqlx::query_as("SELECT user_id,side,answer,paid FROM reef_game_players WHERE game_id=$1 ORDER BY side")
                .bind(id)
                .fetch_all(&mut *tx)
                .await?;
        try_lock_users(&mut tx, &players.iter().map(|p| p.user_id).collect::<Vec<_>>()).await?;
        if let Some(old) = sqlx::query_scalar::<_, Value>(
            "SELECT payload FROM reef_game_actions WHERE game_id=$1 AND user_id=$2 AND request_id=$3",
        )
        .bind(id)
        .bind(sql_id(user))
        .bind(request)
        .fetch_optional(&mut *tx)
        .await?
        {
            anyhow::ensure!(old == *payload, "request_conflict");
            return Ok(game);
        }
        let now: i64 = sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp()))::bigint")
            .fetch_one(&mut *tx)
            .await?;
        let next = step(&game, &players, now)?;
        if next.join {
            let side = (0_i16..32)
                .find(|side| !players.iter().any(|p| p.side == *side))
                .ok_or_else(|| anyhow::anyhow!("room_busy"))?;
            sqlx::query("INSERT INTO reef_game_players(game_id,user_id,side) VALUES($1,$2,$3)")
                .bind(id)
                .bind(sql_id(user))
                .bind(side)
                .execute(&mut *tx)
                .await?;
        }
        if let Some(answer) = next.answer {
            let changed = sqlx::query(
                "UPDATE reef_game_players SET answer=$3 WHERE game_id=$1 AND user_id=$2 AND answer IS NULL",
            )
            .bind(id)
            .bind(sql_id(user))
            .bind(answer)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            anyhow::ensure!(changed == 1, "already_answered");
        }
        if next.finished && !game.finished && game.state["phase"] == "active" {
            for p in &players {
                let won = next.winners.contains(&p.side);
                game_stat(&mut tx, p.user_id, &game.kind, won).await?;
                let paid = if won {
                    capped_win(
                        &mut tx,
                        p.user_id,
                        &game.kind,
                        next.reward,
                        next.cap,
                        &format!("{}_win", game.kind),
                    )
                    .await?
                } else {
                    0
                };
                sqlx::query("UPDATE reef_game_players SET paid=$3 WHERE game_id=$1 AND user_id=$2")
                    .bind(id)
                    .bind(p.user_id)
                    .bind(paid)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        let updated=sqlx::query_as("UPDATE reef_games SET state=$2,finished=$3,revision=revision+1,placement_id=CASE WHEN EXISTS(SELECT 1 FROM placements WHERE id=reef_games.placement_id) THEN placement_id ELSE NULL END,deadline=CASE WHEN $4>0 THEN to_timestamp($5::double precision)+make_interval(secs=>$4) ELSE deadline END WHERE id=$1 RETURNING id,room_key,owner_id,host_id,kind,placement_id,layout_revision,state,revision,finished,deadline")
            .bind(id).bind(next.state).bind(next.finished).bind(next.seconds as f64).bind(now as f64).fetch_one(&mut *tx).await?;
        sqlx::query("INSERT INTO reef_game_actions(game_id,user_id,request_id,payload) VALUES($1,$2,$3,$4)")
            .bind(id)
            .bind(sql_id(user))
            .bind(request)
            .bind(payload)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(updated)
    }
}
