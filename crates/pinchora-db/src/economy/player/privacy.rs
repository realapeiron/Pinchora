use super::*;

/// All counterpart locks were collected by privacy::others_written before object rows.
pub(crate) async fn privacy_refund(tx: &mut Tx<'_>, user: i64, now: DateTime<Utc>) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO economy_limits(user_id,day,last_shift) SELECT $1,$2,max(last_shift) FROM economy_limits WHERE user_id=$1 ON CONFLICT(user_id,day) DO UPDATE SET last_shift=GREATEST(economy_limits.last_shift,EXCLUDED.last_shift)").bind(user).bind((now.timestamp()/86400) as i32).execute(&mut **tx).await?;
    let rows: Vec<Auction> = sqlx::query_as(
        "SELECT * FROM economy_auctions WHERE status='open' AND (seller_id=$1 OR bidder_id=$1) ORDER BY id FOR UPDATE",
    )
    .bind(user)
    .fetch_all(&mut **tx)
    .await?;
    for a in rows {
        if let Some(b) = a.bidder_id
            && b != user
        {
            money(tx, b, a.bid, "auction_refund").await?;
        }
        if a.seller_id != user {
            inventory_credit(tx, a.seller_id, &a.item_id, 1, "auction_return").await?;
        }
    }
    let rows: Vec<Trade> = sqlx::query_as(
        "SELECT * FROM economy_trades WHERE status='open' AND (user_a=$1 OR user_b=$1) ORDER BY id FOR UPDATE",
    )
    .bind(user)
    .fetch_all(&mut **tx)
    .await?;
    for t in rows {
        if t.user_a != user {
            give_offer(tx, t.user_a, &serde_json::from_value(t.offer_a)?, "trade_refund").await?;
        }
        if t.user_b != user && t.held_b {
            give_offer(tx, t.user_b, &serde_json::from_value(t.offer_b)?, "trade_refund").await?;
        }
    }
    Ok(())
}
