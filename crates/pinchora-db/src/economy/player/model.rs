use super::*;

/// Audit identity for an admin mutation; the change and audit row share one commit.
pub struct EconomyAudit<'a> {
    pub actor: u64,
    pub name: &'a str,
    pub source: crate::AuditSource,
}
pub(super) async fn admin_audit(
    tx: &mut Tx<'_>,
    guild: u64,
    actor: Option<&EconomyAudit<'_>>,
    action: &str,
    before: Value,
    after: Value,
) -> anyhow::Result<()> {
    if let Some(actor) = actor {
        crate::moderation::audit::insert_in(
            tx,
            &crate::NewAudit {
                guild,
                actor: actor.actor,
                actor_name: actor.name,
                source: actor.source,
                action,
                target: None,
                before: Some(&before.to_string()),
                after: Some(&after.to_string()),
            },
        )
        .await?;
    }
    Ok(())
}

pub(super) type Tx<'a> = Transaction<'a, Postgres>;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    StartShift { job: String },
    Step { token: i64, step: i32 },
    Tend,
    Provisions,
    Chest,
    Season { tier: i32 },
    Prestige { expected: i32 },
    Auction { item: String, price: i64, hours: i32 },
    Bid { id: i64, amount: i64 },
    CancelAuction { id: i64 },
    SettleAuction { id: i64 },
    Trade { to: String, give: Offer, receive: Offer },
    ConfirmTrade { id: i64 },
    CancelTrade { id: i64 },
    Donate { guild: String, amount: i64 },
}
#[derive(Debug)]
pub struct Rejected(pub &'static str);
impl std::fmt::Display for Rejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for Rejected {}
pub(super) fn require(ok: bool, key: &'static str) -> anyhow::Result<()> {
    if ok { Ok(()) } else { Err(Rejected(key).into()) }
}
pub(super) fn id(raw: &str) -> anyhow::Result<i64> {
    raw.parse::<i64>()
        .ok()
        .filter(|x| *x > 0)
        .ok_or_else(|| Rejected("invalid").into())
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Player {
    pub job: String,
    pub shifts: i32,
    pub prestige: i32,
    pub chests: i32,
    pub provision_at: Option<DateTime<Utc>>,
    pub provision_claimed: i32,
    pub shift_token: i64,
    pub shift_step: i32,
    pub shift_started: Option<DateTime<Utc>>,
    pub season: i32,
    pub season_xp: i32,
}
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Auction {
    pub id: i64,
    pub seller_id: i64,
    pub item_id: String,
    pub start_price: i64,
    pub bid: i64,
    pub bidder_id: Option<i64>,
    pub ends_at: DateTime<Utc>,
    pub hard_end: DateTime<Utc>,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Trade {
    pub id: i64,
    pub user_a: i64,
    pub user_b: i64,
    pub offer_a: Value,
    pub offer_b: Value,
    pub confirmed_a: bool,
    pub confirmed_b: bool,
    pub held_b: bool,
    pub expires_at: DateTime<Utc>,
    pub status: String,
}
pub(super) async fn ensure_player(tx: &mut Tx<'_>, user: i64) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO users(id) VALUES($1) ON CONFLICT DO NOTHING")
        .bind(user)
        .execute(&mut **tx)
        .await?;
    sqlx::query("INSERT INTO economy_players(user_id) VALUES($1) ON CONFLICT DO NOTHING")
        .bind(user)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
