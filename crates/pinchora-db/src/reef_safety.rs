//! Reef safety records. Player advisory locks precede evidence/report locks.
use crate::{
    Db,
    locks::{lock_users, retry},
    sql_id,
};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
pub const REEF_CONSENT_VERSION: i32 = 1;

#[derive(sqlx::FromRow)]
pub struct ReefEvidence {
    pub id: i64,
    pub owner_id: i64,
    pub room_idx: i16,
    pub sender_id: i64,
    pub text: String,
    pub created_at: DateTime<Utc>,
}
impl ReefEvidence {
    pub fn json(&self) -> Value {
        json!({"message_id":self.id.to_string(), "room":format!("house:{}:{}",self.owner_id,self.room_idx),
            "sender_id":self.sender_id.to_string(),"text":self.text,"created_at":self.created_at})
    }
}
type ReportEvidenceRow = (
    i64,
    i64,
    String,
    String,
    Option<String>,
    Option<String>,
    i64,
    i16,
    i64,
    String,
    DateTime<Utc>,
);
type ReceiptRow = (i64, String, Option<String>, Option<String>);
impl Db {
    pub async fn reef_eligible(&self, user: u64) -> anyhow::Result<bool> {
        Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM reef_consents WHERE user_id=$1 AND version=$2 AND age_band='13_plus' AND accepted)")
            .bind(sql_id(user)).bind(REEF_CONSENT_VERSION).fetch_one(self.pool()).await?)
    }
    pub async fn reef_consent(&self, user: u64, version: i32, age: &str, accepted: bool) -> anyhow::Result<()> {
        anyhow::ensure!(
            version == REEF_CONSENT_VERSION && ["under_13", "13_plus"].contains(&age),
            "invalid consent"
        );
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        sqlx::query("INSERT INTO reef_consents(user_id,version,age_band,accepted) VALUES($1,$2,$3,$4) ON CONFLICT(user_id) DO UPDATE SET version=$2,age_band=$3,accepted=$4,updated_at=now()")
            .bind(sql_id(user)).bind(version).bind(age).bind(accepted).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    /// General social exclusion, in either direction; partner status cannot bypass it.
    pub async fn socially_blocked(&self, a: u64, b: u64) -> anyhow::Result<bool> {
        Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM guestbook_blocks WHERE (host_id=$1 AND blocked_id=$2) OR (host_id=$2 AND blocked_id=$1))")
            .bind(sql_id(a)).bind(sql_id(b)).fetch_one(self.pool()).await?)
    }
    /// Site, house and room scopes all apply. Kicks prevent immediate reconnect for one minute.
    pub async fn reef_sanctioned(&self, user: u64, owner: u64, idx: i16, speaking: bool) -> anyhow::Result<bool> {
        Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM reef_sanctions WHERE user_id=$1 AND expires_at>now() AND (owner_id IS NULL OR owner_id=$2) AND (room_idx IS NULL OR room_idx=$3) AND ($4 OR kind IN ('kick','ban')))")
            .bind(sql_id(user)).bind(sql_id(owner)).bind(idx).bind(speaking).fetch_one(self.pool()).await?)
    }
    /// Persist only accepted chat, and only recipients authorized in this transaction.
    pub async fn reef_chat(
        &self,
        owner: u64,
        idx: i16,
        sender: u64,
        text: &str,
        peers: &[u64],
    ) -> anyhow::Result<Result<(ReefEvidence, Vec<u64>), &'static str>> {
        self.reef_chat_fenced(owner, idx, sender, text, peers, None).await
    }
    /// The fence is locked in the evidence transaction, before checking access or writing chat.
    #[allow(clippy::too_many_arguments)]
    pub async fn reef_chat_fenced(
        &self,
        owner: u64,
        idx: i16,
        sender: u64,
        text: &str,
        peers: &[u64],
        fence: Option<&crate::ReefFence>,
    ) -> anyhow::Result<Result<(ReefEvidence, Vec<u64>), &'static str>> {
        let mut ids: Vec<i64> = peers.iter().copied().map(sql_id).collect();
        ids.extend([sql_id(owner), sql_id(sender)]);
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &ids).await?;
        if let Some(fence) = fence {
            let valid: Option<i64> = sqlx::query_scalar("SELECT token FROM reef_room_leases WHERE namespace=$1 AND owner_id=$2 AND room_idx=$3 AND token=$4 AND expires_at>clock_timestamp() FOR UPDATE")
                .bind(&fence.namespace).bind(sql_id(owner)).bind(idx).bind(fence.token).fetch_optional(&mut *tx).await?;
            if valid.is_none() {
                return Ok(Err("ownership_lost"));
            }
        }
        let accessible: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM houses h JOIN rooms r ON r.user_id=h.user_id WHERE h.user_id=$1 AND r.idx=$2 AND ($1=$3 OR (NOT h.locked AND NOT EXISTS(SELECT 1 FROM user_privacy p WHERE p.user_id=$1 AND p.hide_house)))) AND NOT EXISTS(SELECT 1 FROM guestbook_blocks WHERE (host_id=$1 AND blocked_id=$3) OR (host_id=$3 AND blocked_id=$1))")
            .bind(sql_id(owner)).bind(idx).bind(sql_id(sender)).fetch_one(&mut *tx).await?;
        if !accessible {
            return Ok(Err("access_denied"));
        }
        let eligible: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM reef_consents WHERE user_id=$1 AND version=$2 AND age_band='13_plus' AND accepted)")
            .bind(sql_id(sender)).bind(REEF_CONSENT_VERSION).fetch_one(&mut *tx).await?;
        if !eligible {
            return Ok(Err("eligibility_required"));
        }
        let muted: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM reef_sanctions WHERE user_id=$1 AND expires_at>now() AND (owner_id IS NULL OR owner_id=$2) AND (room_idx IS NULL OR room_idx=$3))")
            .bind(sql_id(sender)).bind(sql_id(owner)).bind(idx).fetch_one(&mut *tx).await?;
        if muted {
            return Ok(Err("chat_muted"));
        }
        let evidence: ReefEvidence = sqlx::query_as("INSERT INTO reef_chat_evidence(owner_id,room_idx,sender_id,text) VALUES($1,$2,$3,$4) RETURNING id,owner_id,room_idx,sender_id,text,created_at")
            .bind(sql_id(owner)).bind(idx).bind(sql_id(sender)).bind(text).fetch_one(&mut *tx).await?;
        let recipients: Vec<i64> = sqlx::query_scalar("INSERT INTO reef_chat_recipients(message_id,user_id) SELECT $1, p FROM unnest($2::bigint[]) p WHERE EXISTS(SELECT 1 FROM reef_consents c WHERE c.user_id=p AND c.version=$4 AND c.accepted AND c.age_band='13_plus') AND NOT EXISTS(SELECT 1 FROM guestbook_blocks b WHERE (b.host_id=p AND b.blocked_id=$3) OR (b.host_id=$3 AND b.blocked_id=p)) RETURNING user_id")
            .bind(evidence.id).bind(peers.iter().copied().map(sql_id).collect::<Vec<_>>()).bind(sql_id(sender)).bind(REEF_CONSENT_VERSION).fetch_all(&mut *tx).await?;
        tx.commit().await?;
        Ok(Ok((evidence, recipients.into_iter().map(|id| id as u64).collect())))
    }
    pub async fn reef_report(&self, user: u64, message: i64, reason: &str) -> anyhow::Result<Option<String>> {
        if !["harassment", "personal_info", "spam", "other"].contains(&reason) {
            return Ok(None);
        }
        let mut tx = self.pool().begin().await?;
        let authors: Option<(i64, i64)> =
            sqlx::query_as("SELECT owner_id,sender_id FROM reef_chat_evidence WHERE id=$1")
                .bind(message)
                .fetch_optional(&mut *tx)
                .await?;
        let Some((owner, sender)) = authors else {
            return Ok(None);
        };
        lock_users(&mut tx, &[sql_id(user), owner, sender]).await?;
        // Taking the message lock serializes reporting with the retention sweep.
        let found: Option<i64> = sqlx::query_scalar("SELECT e.id FROM reef_chat_evidence e WHERE e.id=$1 AND EXISTS(SELECT 1 FROM reef_chat_recipients r WHERE r.message_id=e.id AND r.user_id=$2) FOR UPDATE OF e")
            .bind(message).bind(sql_id(user)).fetch_optional(&mut *tx).await?;
        if found.is_none() {
            return Ok(None);
        }
        let id: i64 = sqlx::query_scalar("INSERT INTO reef_reports(message_id,reporter_id,reason) VALUES($1,$2,$3) ON CONFLICT(message_id,reporter_id) DO UPDATE SET message_id=EXCLUDED.message_id RETURNING id")
            .bind(message).bind(sql_id(user)).bind(reason).fetch_one(&mut *tx).await?;
        sqlx::query("UPDATE reef_chat_evidence SET reported=TRUE WHERE id=$1")
            .bind(message)
            .execute(&mut *tx)
            .await?;
        audit(&mut tx, user, None, "report", Some(id)).await?;
        tx.commit().await?;
        Ok(Some(id.to_string()))
    }
    /// Restricted report evidence only, never a general chat-history endpoint. Each read is audited.
    pub async fn reef_queue(&self, actor: u64, operator: bool) -> anyhow::Result<Vec<Value>> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(actor)]).await?;
        let rows: Vec<ReportEvidenceRow> = sqlx::query_as("SELECT r.id,r.message_id,r.reason,r.status,r.disposition,r.appeal,e.owner_id,e.room_idx,e.sender_id,e.text,e.created_at FROM reef_reports r JOIN reef_chat_evidence e ON e.id=r.message_id WHERE ($2 OR e.owner_id=$1) AND r.status IN ('open','appealed') ORDER BY r.id LIMIT 100")
            .bind(sql_id(actor)).bind(operator).fetch_all(&mut *tx).await?;
        let mut items = vec![];
        for (id, message, reason, status, disposition, appeal, owner, idx, sender, text, created) in rows {
            audit(&mut tx, actor, None, "evidence_read", Some(id)).await?;
            items.push(json!({"id":id.to_string(),"message_id":message.to_string(),"reason":reason,"status":status,"disposition":disposition,"appeal":appeal,"room":format!("house:{owner}:{idx}"),"sender_id":sender.to_string(),"text":text,"created_at":created}));
        }
        tx.commit().await?;
        Ok(items)
    }
    pub async fn reef_receipts(&self, user: u64) -> anyhow::Result<Vec<Value>> {
        let rows: Vec<ReceiptRow> = sqlx::query_as("SELECT r.id,r.status,r.disposition,r.appeal FROM reef_reports r JOIN reef_chat_evidence e ON e.id=r.message_id WHERE r.reporter_id=$1 OR e.sender_id=$1 ORDER BY r.id DESC LIMIT 100")
            .bind(sql_id(user)).fetch_all(self.pool()).await?;
        Ok(rows.into_iter().map(|(id,status,disposition,appeal)| json!({"id":id.to_string(),"status":status,"disposition":disposition,"appeal":appeal})).collect())
    }
    pub async fn reef_appeal(&self, user: u64, report: i64, text: &str) -> anyhow::Result<bool> {
        let mut tx = self.pool().begin().await?;
        let reporter: Option<i64> = sqlx::query_scalar("SELECT reporter_id FROM reef_reports WHERE id=$1")
            .bind(report)
            .fetch_optional(&mut *tx)
            .await?;
        let Some(reporter) = reporter else { return Ok(false) };
        lock_users(&mut tx, &[sql_id(user), reporter]).await?;
        let count = sqlx::query("UPDATE reef_reports r SET appeal=$3,status='appealed' FROM reef_chat_evidence e WHERE r.id=$1 AND e.id=r.message_id AND (r.reporter_id=$2 OR e.sender_id=$2) AND r.status='resolved'")
            .bind(report).bind(sql_id(user)).bind(text).execute(&mut *tx).await?.rows_affected();
        if count > 0 {
            audit(&mut tx, user, None, "appeal", Some(report)).await?;
        }
        tx.commit().await?;
        Ok(count > 0)
    }
    /// Permission is checked both by the route and here; report disposition and action are atomic.
    pub async fn reef_moderate(&self, action: &ReefModeration) -> anyhow::Result<bool> {
        let a = action;
        if !["mute", "kick", "ban", "no_action", "warned", "revoke"].contains(&a.kind.as_str())
            || (!a.operator && a.owner != Some(a.actor))
            || a.target == a.actor
            || !(1..=90 * 86400).contains(&a.seconds)
            || a.idx.is_some_and(|i| !(0..6).contains(&i))
            || (a.owner.is_none() && a.idx.is_some())
        {
            return Ok(false);
        }
        let mut tx = self.pool().begin().await?;
        let mut ids = vec![sql_id(a.actor), sql_id(a.target)];
        ids.extend(a.owner.map(sql_id));
        if let Some(report) = a.report {
            let reporter: Option<i64> = sqlx::query_scalar("SELECT reporter_id FROM reef_reports WHERE id=$1")
                .bind(report)
                .fetch_optional(&mut *tx)
                .await?;
            ids.extend(reporter);
        }
        lock_users(&mut tx, &ids).await?;
        if let Some(report) = a.report {
            let allowed: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM reef_reports r JOIN reef_chat_evidence e ON e.id=r.message_id WHERE r.id=$1 AND e.sender_id=$2 AND ($3 OR e.owner_id=$4) AND ($5::bigint IS NULL OR e.owner_id=$5) AND ($6::smallint IS NULL OR e.room_idx=$6))")
                .bind(report).bind(sql_id(a.target)).bind(a.operator).bind(sql_id(a.actor)).bind(a.owner.map(sql_id)).bind(a.idx).fetch_one(&mut *tx).await?;
            if !allowed {
                return Ok(false);
            }
            sqlx::query("UPDATE reef_reports SET status='resolved',disposition=$2 WHERE id=$1")
                .bind(report)
                .bind(if a.kind == "revoke" { "no_action" } else { &a.kind })
                .execute(&mut *tx)
                .await?;
        }
        if a.kind == "revoke" {
            sqlx::query("DELETE FROM reef_sanctions WHERE user_id=$1 AND owner_id IS NOT DISTINCT FROM $2 AND room_idx IS NOT DISTINCT FROM $3")
                .bind(sql_id(a.target)).bind(a.owner.map(sql_id)).bind(a.idx).execute(&mut *tx).await?;
        }
        if ["mute", "kick", "ban"].contains(&a.kind.as_str()) {
            sqlx::query("INSERT INTO reef_sanctions(user_id,actor_id,owner_id,room_idx,kind,expires_at) VALUES($1,$2,$3,$4,$5,now()+$6::integer * interval '1 second')")
                .bind(sql_id(a.target)).bind(sql_id(a.actor)).bind(a.owner.map(sql_id)).bind(a.idx).bind(&a.kind).bind(if a.kind=="kick" {60} else {a.seconds}).execute(&mut *tx).await?;
        }
        audit(&mut tx, a.actor, Some(a.target), &a.kind, a.report).await?;
        tx.commit().await?;
        Ok(true)
    }
    pub async fn reef_retain(&self, now: DateTime<Utc>) -> anyhow::Result<u64> {
        retry("reef_retention", || async {
            let mut tx = self.pool().begin().await?;
            let count=sqlx::query("DELETE FROM reef_chat_evidence WHERE id IN (SELECT id FROM reef_chat_evidence WHERE created_at < $1 - CASE WHEN reported THEN interval '90 days' ELSE interval '7 days' END ORDER BY id LIMIT 500 FOR UPDATE SKIP LOCKED)")
                .bind(now).execute(&mut *tx).await?.rows_affected();
            sqlx::query("DELETE FROM reef_sanctions WHERE id IN (SELECT id FROM reef_sanctions WHERE expires_at <= $1 LIMIT 500 FOR UPDATE SKIP LOCKED)").bind(now).execute(&mut *tx).await?;
            sqlx::query("DELETE FROM reef_moderator_audit WHERE id IN (SELECT id FROM reef_moderator_audit WHERE created_at < $1 - interval '90 days' LIMIT 500 FOR UPDATE SKIP LOCKED)").bind(now).execute(&mut *tx).await?;
            // System actor 0; counts only, never message text or credentials.
            if count>0 { audit(&mut tx,0,None,"retention",Some(count as i64)).await?; }
            tx.commit().await?;
            Ok(count)
        }).await
    }
}
pub struct ReefModeration {
    pub actor: u64,
    pub operator: bool,
    pub target: u64,
    pub owner: Option<u64>,
    pub idx: Option<i16>,
    pub kind: String,
    pub seconds: i32,
    pub report: Option<i64>,
}
async fn audit(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor: u64,
    target: Option<u64>,
    action: &str,
    subject: Option<i64>,
) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO reef_moderator_audit(actor_id,target_user_id,action,subject_id) VALUES($1,$2,$3,$4)")
        .bind(sql_id(actor))
        .bind(target.map(sql_id))
        .bind(action)
        .bind(subject)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
