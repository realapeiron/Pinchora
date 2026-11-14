use super::*;

/// The two sides are inserted together; every balance/inventory write and its pair commit together.
pub(crate) async fn movement(tx: &mut Tx<'_>, user: i64, asset: &str, delta: i64, reason: &str) -> anyhow::Result<()> {
    if delta == 0 {
        return Ok(());
    }
    let account = match reason {
        "job_shift" | "provisions" | "earned_chest" | "season_pass" | "job_chest" | "collection_reward" => "source",
        "donation_pool" | "prestige" | "auction_fee" | "chest_open" | "collection_upgrade" => "sink",
        "collection_recycle" | "collection_craft" | "collection_exchange" if delta > 0 => "source",
        "collection_recycle" | "collection_craft" | "collection_exchange" => "sink",
        _ => "escrow",
    };
    sqlx::query("WITH m AS MATERIALIZED (SELECT nextval('economy_movement_ids') AS id) INSERT INTO economy_ledger(movement,user_id,account,asset,delta,reason) SELECT id,$1,'wallet',$2,$3,$4 FROM m UNION ALL SELECT id,$1,$5,$2,-$3,$4 FROM m")
        .bind(user).bind(asset).bind(delta).bind(reason).bind(account).execute(&mut **tx).await?;
    Ok(())
}
pub(super) async fn money_record(tx: &mut Tx<'_>, user: i64, delta: i64, reason: &str) -> anyhow::Result<()> {
    if delta == 0 {
        return Ok(());
    }
    ledger(tx, user, delta, reason).await?;
    movement(tx, user, "shells", delta, reason).await
}
/// Numeric comparison prevents SQL overflow before evaluating the guarded addition.
pub(super) async fn balance(tx: &mut Tx<'_>, user: i64, delta: i64) -> anyhow::Result<()> {
    let n = sqlx::query(
        "UPDATE users SET credits=credits+$2 WHERE id=$1 AND credits::numeric+$2 BETWEEN 0 AND 9223372036854775807",
    )
    .bind(user)
    .bind(delta)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    require(n == 1, "balance")
}
pub(super) async fn money(tx: &mut Tx<'_>, user: i64, delta: i64, reason: &str) -> anyhow::Result<()> {
    if delta == 0 {
        return Ok(());
    }
    balance(tx, user, delta).await?;
    money_record(tx, user, delta, reason).await
}
pub(super) async fn inventory_credit(
    tx: &mut Tx<'_>,
    user: i64,
    item: &str,
    quantity: i32,
    reason: &str,
) -> anyhow::Result<()> {
    let n = sqlx::query("INSERT INTO inventory(user_id,item_id,quantity) VALUES($1,$2,$3) ON CONFLICT(user_id,item_id) DO UPDATE SET quantity=inventory.quantity+EXCLUDED.quantity,acquired_at=now() WHERE inventory.quantity::bigint+EXCLUDED.quantity<=2147483647")
        .bind(user).bind(item).bind(quantity).execute(&mut **tx).await?.rows_affected();
    require(n == 1, "inventory")?;
    movement(tx, user, &format!("item:{item}"), i64::from(quantity), reason).await
}
pub(super) async fn take_offer(tx: &mut Tx<'_>, user: i64, offer: &Offer, reason: &str) -> anyhow::Result<()> {
    require(offer.valid(), "invalid")?;
    money(tx, user, -offer.shells, reason).await?;
    for i in &offer.items {
        let n =
            sqlx::query("UPDATE inventory SET quantity=quantity-$3 WHERE user_id=$1 AND item_id=$2 AND quantity >= $3")
                .bind(user)
                .bind(&i.item_id)
                .bind(i.quantity)
                .execute(&mut **tx)
                .await?
                .rows_affected();
        require(n == 1, "inventory")?;
        movement(tx, user, &format!("item:{}", i.item_id), -i64::from(i.quantity), reason).await?;
    }
    Ok(())
}
pub(super) async fn give_offer(tx: &mut Tx<'_>, user: i64, offer: &Offer, reason: &str) -> anyhow::Result<()> {
    money(tx, user, offer.shells, reason).await?;
    for i in &offer.items {
        inventory_credit(tx, user, &i.item_id, i.quantity, reason).await?;
    }
    Ok(())
}
pub(super) async fn auction(tx: &mut Tx<'_>, id: i64) -> anyhow::Result<Auction> {
    sqlx::query_as("SELECT * FROM economy_auctions WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| Rejected("gone").into())
}
pub(super) async fn trade(tx: &mut Tx<'_>, id: i64) -> anyhow::Result<Trade> {
    sqlx::query_as("SELECT * FROM economy_trades WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| Rejected("gone").into())
}
