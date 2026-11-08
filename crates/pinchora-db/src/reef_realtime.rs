//! Durable fencing and layout notifications. Presence remains ephemeral.
use crate::{Db, sql_id};

#[derive(Clone)]
pub struct ReefFence {
    pub namespace: String,
    pub owner: u64,
    pub idx: i16,
    pub token: i64,
}
impl Db {
    pub async fn reef_acquire(&self, namespace: &str, owner: u64, idx: i16) -> anyhow::Result<Option<ReefFence>> {
        let token = sqlx::query_scalar("INSERT INTO reef_room_leases(namespace,owner_id,room_idx,expires_at) VALUES($1,$2,$3,clock_timestamp()+interval '3 seconds') ON CONFLICT(namespace,owner_id,room_idx) DO UPDATE SET token=nextval('reef_fencing_token'),expires_at=clock_timestamp()+interval '3 seconds' WHERE reef_room_leases.expires_at<clock_timestamp() RETURNING token")
            .bind(namespace).bind(sql_id(owner)).bind(idx).fetch_optional(self.pool()).await?;
        Ok(token.map(|token| ReefFence {
            namespace: namespace.into(),
            owner,
            idx,
            token,
        }))
    }
    pub async fn reef_renew(&self, fence: &ReefFence) -> anyhow::Result<bool> {
        let mut tx = self.pool().begin().await?;
        if !fence.lock_valid(&mut tx).await? {
            return Ok(false);
        }
        sqlx::query("UPDATE reef_room_leases SET expires_at=clock_timestamp()+interval '3 seconds' WHERE namespace=$1 AND owner_id=$2 AND room_idx=$3 AND token=$4")
            .bind(&fence.namespace).bind(sql_id(fence.owner)).bind(fence.idx).bind(fence.token).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(true)
    }
    pub async fn reef_release(&self, fence: &ReefFence) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM reef_room_leases WHERE namespace=$1 AND owner_id=$2 AND room_idx=$3 AND token=$4")
            .bind(&fence.namespace)
            .bind(sql_id(fence.owner))
            .bind(fence.idx)
            .bind(fence.token)
            .execute(self.pool())
            .await?;
        Ok(())
    }
    /// Scan committed IDs rather than an ID high-water mark: sequence allocation is not commit order.
    pub async fn reef_layout_events(&self, owner: u64) -> anyhow::Result<Vec<i64>> {
        Ok(sqlx::query_scalar("SELECT id FROM reef_layout_outbox WHERE owner_id=$1 AND created_at>clock_timestamp()-interval '1 day' ORDER BY id")
            .bind(sql_id(owner)).fetch_all(self.pool()).await?)
    }
    pub async fn reef_prune_outbox(&self) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM reef_edit_receipts WHERE (actor_id,request_id) IN (SELECT actor_id,request_id FROM reef_edit_receipts WHERE created_at<now()-interval '7 days' ORDER BY created_at LIMIT 500 FOR UPDATE SKIP LOCKED)")
            .execute(self.pool())
            .await?;
        sqlx::query("DELETE FROM reef_doorbells WHERE id IN (SELECT id FROM reef_doorbells WHERE expires_at<now()-interval '1 day' ORDER BY expires_at LIMIT 500 FOR UPDATE SKIP LOCKED)")
            .execute(self.pool())
            .await?;
        sqlx::query("DELETE FROM reef_layout_outbox WHERE id IN (SELECT id FROM reef_layout_outbox WHERE created_at<clock_timestamp()-interval '1 day' LIMIT 500)").execute(self.pool()).await?;
        sqlx::query("DELETE FROM reef_room_leases WHERE expires_at<clock_timestamp()-interval '1 day'")
            .execute(self.pool())
            .await?;
        Ok(())
    }
}

impl ReefFence {
    pub(crate) async fn lock_valid(&self, tx: &mut sqlx::Transaction<'_, sqlx::Postgres>) -> anyhow::Result<bool> {
        // A WHERE predicate can run before a row-lock wait. Check the clock in a separate
        // statement after acquiring the lock, so waiting never resurrects an expired owner.
        let found: Option<i64> = sqlx::query_scalar("SELECT token FROM reef_room_leases WHERE namespace=$1 AND owner_id=$2 AND room_idx=$3 AND token=$4 FOR UPDATE")
            .bind(&self.namespace).bind(sql_id(self.owner)).bind(self.idx).bind(self.token).fetch_optional(&mut **tx).await?;
        if found.is_none() {
            return Ok(false);
        }
        Ok(sqlx::query_scalar("SELECT expires_at>clock_timestamp() FROM reef_room_leases WHERE namespace=$1 AND owner_id=$2 AND room_idx=$3 AND token=$4")
            .bind(&self.namespace).bind(sql_id(self.owner)).bind(self.idx).bind(self.token).fetch_one(&mut **tx).await?)
    }
}
