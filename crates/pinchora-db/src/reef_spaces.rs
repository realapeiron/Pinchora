//! Wave 7 editing receipts and consent-controlled house admission.
use crate::{Db, locks::lock_users, sql_id};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct ReefEditRequest {
    pub actor: u64,
    pub request: String,
    pub payload: String,
    pub revision: i64,
}
impl ReefEditRequest {
    pub(crate) async fn check(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        owner: u64,
    ) -> anyhow::Result<bool> {
        let revision: i64 = sqlx::query_scalar("SELECT edit_revision FROM houses WHERE user_id=$1")
            .bind(sql_id(owner))
            .fetch_one(&mut **tx)
            .await?;
        Ok(revision == self.revision)
    }
    pub(crate) async fn save(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        owner: u64,
        room: i16,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO reef_edit_receipts(actor_id,request_id,owner_id,room_idx,payload) VALUES($1,$2,$3,$4,$5)",
        )
        .bind(sql_id(self.actor))
        .bind(&self.request)
        .bind(sql_id(owner))
        .bind(room)
        .bind(&self.payload)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, sqlx::FromRow)]
#[serde(deny_unknown_fields)]
pub struct ReefSpacePreferences {
    pub presence: bool,
    pub visits: String,
    pub doorbell: bool,
}
impl Db {
    pub async fn reef_edit_revision(&self, owner: u64) -> anyhow::Result<String> {
        let n: i64 = sqlx::query_scalar("SELECT edit_revision FROM houses WHERE user_id=$1")
            .bind(sql_id(owner))
            .fetch_one(self.pool())
            .await?;
        Ok(n.to_string())
    }
    /// None is new, true is a retry, false is reuse of an id for a different intention.
    pub async fn reef_edit_receipt(
        &self,
        owner: u64,
        room: i16,
        request: &ReefEditRequest,
    ) -> anyhow::Result<Option<bool>> {
        let row: Option<(i64, i16, String)> = sqlx::query_as(
            "SELECT owner_id,room_idx,payload FROM reef_edit_receipts WHERE actor_id=$1 AND request_id=$2",
        )
        .bind(sql_id(request.actor))
        .bind(&request.request)
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|(o, r, p)| o == sql_id(owner) && r == room && p == request.payload))
    }
    pub async fn reef_space_preferences(&self, user: u64) -> anyhow::Result<ReefSpacePreferences> {
        Ok(
            sqlx::query_as("SELECT presence,visits,doorbell FROM reef_space_preferences WHERE user_id=$1")
                .bind(sql_id(user))
                .fetch_optional(self.pool())
                .await?
                .unwrap_or(ReefSpacePreferences {
                    presence: false,
                    visits: "open".into(),
                    doorbell: false,
                }),
        )
    }
    pub async fn reef_save_space_preferences(&self, user: u64, p: &ReefSpacePreferences) -> anyhow::Result<()> {
        anyhow::ensure!(
            ["open", "friends", "doorbell", "closed"].contains(&p.visits.as_str()),
            "invalid preferences"
        );
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        sqlx::query("INSERT INTO reef_space_preferences(user_id,presence,visits,doorbell) VALUES($1,$2,$3,$4) ON CONFLICT(user_id) DO UPDATE SET presence=$2,visits=$3,doorbell=$4")
            .bind(sql_id(user)).bind(p.presence).bind(&p.visits).bind(p.doorbell).execute(&mut *tx).await?;
        if !p.doorbell || p.visits == "closed" {
            sqlx::query("DELETE FROM reef_doorbells WHERE host_id=$1")
                .bind(sql_id(user))
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }
    pub async fn reef_house_access(&self, host: u64, guest: u64) -> anyhow::Result<bool> {
        Ok(sqlx::query_scalar("SELECT reef_house_admitted($1,$2)")
            .bind(sql_id(host))
            .bind(sql_id(guest))
            .fetch_one(self.pool())
            .await?)
    }
    pub async fn reef_admitted(&self, host: u64, guest: u64) -> anyhow::Result<bool> {
        Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM reef_doorbells WHERE host_id=$1 AND guest_id=$2 AND status='accepted' AND expires_at>now())")
            .bind(sql_id(host)).bind(sql_id(guest)).fetch_one(self.pool()).await?)
    }
    pub async fn reef_ring(&self, host: u64, guest: u64) -> anyhow::Result<Option<String>> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(host), sql_id(guest)]).await?;
        let allowed:bool=sqlx::query_scalar("SELECT $1<>$2 AND EXISTS(SELECT 1 FROM houses WHERE user_id=$1) AND NOT EXISTS(SELECT 1 FROM user_privacy WHERE user_id=$1 AND hide_house) AND EXISTS(SELECT 1 FROM reef_space_preferences WHERE user_id=$1 AND doorbell AND visits<>'closed') AND NOT EXISTS(SELECT 1 FROM guestbook_blocks WHERE (host_id=$1 AND blocked_id=$2) OR (host_id=$2 AND blocked_id=$1)) AND NOT EXISTS(SELECT 1 FROM reef_doorbells WHERE host_id=$1 AND guest_id=$2 AND created_at>now()-interval '1 minute') AND (SELECT count(*) FROM reef_doorbells WHERE guest_id=$2 AND created_at>now()-interval '1 minute')<6 AND (SELECT count(*) FROM reef_doorbells WHERE host_id=$1 AND status='pending' AND expires_at>now())<16")
            .bind(sql_id(host)).bind(sql_id(guest)).fetch_one(&mut *tx).await?;
        if !allowed {
            return Ok(None);
        }
        let id: i64 = sqlx::query_scalar("INSERT INTO reef_doorbells(host_id,guest_id) VALUES($1,$2) RETURNING id")
            .bind(sql_id(host))
            .bind(sql_id(guest))
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Some(id.to_string()))
    }
    pub async fn reef_decide_doorbell(&self, host: u64, id: i64, accept: bool) -> anyhow::Result<bool> {
        let mut tx = self.pool().begin().await?;
        let guest: Option<i64> = sqlx::query_scalar("SELECT guest_id FROM reef_doorbells WHERE id=$1 AND host_id=$2")
            .bind(id)
            .bind(sql_id(host))
            .fetch_optional(&mut *tx)
            .await?;
        let Some(guest) = guest else { return Ok(false) };
        lock_users(&mut tx, &[sql_id(host), guest]).await?;
        let changed=sqlx::query("UPDATE reef_doorbells SET status=$3,expires_at=now()+interval '10 minutes' WHERE id=$1 AND host_id=$2 AND status='pending' AND expires_at>now() AND NOT EXISTS(SELECT 1 FROM user_privacy WHERE user_id=$2 AND hide_house) AND EXISTS(SELECT 1 FROM reef_space_preferences WHERE user_id=$2 AND doorbell AND visits<>'closed') AND NOT EXISTS(SELECT 1 FROM guestbook_blocks WHERE (host_id=$2 AND blocked_id=$4) OR (host_id=$4 AND blocked_id=$2))")
            .bind(id).bind(sql_id(host)).bind(if accept {"accepted"} else {"declined"}).bind(guest).execute(&mut *tx).await?.rows_affected()>0;
        tx.commit().await?;
        Ok(changed)
    }
    pub async fn reef_doorbells(&self, user: u64) -> anyhow::Result<Vec<serde_json::Value>> {
        let rows:Vec<(i64,i64,i64,String,chrono::DateTime<chrono::Utc>)>=sqlx::query_as("SELECT id,host_id,guest_id,status,expires_at FROM reef_doorbells WHERE (host_id=$1 OR guest_id=$1) AND expires_at>now() AND NOT EXISTS(SELECT 1 FROM guestbook_blocks b WHERE (b.host_id=reef_doorbells.host_id AND b.blocked_id=reef_doorbells.guest_id) OR (b.host_id=reef_doorbells.guest_id AND b.blocked_id=reef_doorbells.host_id)) AND NOT EXISTS(SELECT 1 FROM user_privacy p WHERE p.user_id=reef_doorbells.host_id AND p.hide_house) ORDER BY id DESC LIMIT 32").bind(sql_id(user)).fetch_all(self.pool()).await?;
        Ok(rows.into_iter().map(|(id,h,g,s,e)|serde_json::json!({"id":id.to_string(),"host_id":h.to_string(),"guest_id":g.to_string(),"status":s,"expires_at":e})).collect())
    }
    pub async fn reef_note_evidence(&self, host: u64, reader: u64, note: i64) -> anyhow::Result<Option<i64>> {
        let mut tx = self.pool().begin().await?;
        let author: Option<i64> =
            sqlx::query_scalar("SELECT author_id FROM guestbook_notes WHERE id=$1 AND host_id=$2")
                .bind(note)
                .bind(sql_id(host))
                .fetch_optional(&mut *tx)
                .await?;
        let Some(author) = author else { return Ok(None) };
        lock_users(&mut tx, &[sql_id(host), sql_id(reader), author]).await?;
        let id:Option<i64>=sqlx::query_scalar("INSERT INTO reef_chat_evidence(owner_id,room_idx,sender_id,text,guestbook_note_id) SELECT host_id,0,author_id,body,id FROM guestbook_notes WHERE id=$1 AND host_id=$2 AND reef_house_admitted($2,$3) ON CONFLICT(guestbook_note_id) WHERE guestbook_note_id IS NOT NULL DO UPDATE SET guestbook_note_id=EXCLUDED.guestbook_note_id RETURNING id")
            .bind(note).bind(sql_id(host)).bind(sql_id(reader)).fetch_optional(&mut *tx).await?;
        if let Some(id) = id {
            sqlx::query("INSERT INTO reef_chat_recipients(message_id,user_id) VALUES($1,$2) ON CONFLICT DO NOTHING")
                .bind(id)
                .bind(sql_id(reader))
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(id)
    }
}
