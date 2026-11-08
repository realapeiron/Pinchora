//! Checks run under the opener and guild opening locks, in the ticket insertion transaction.
use crate::{NewTicket, TicketOpen, sql_id};
use sqlx::{Postgres, Transaction};

pub(crate) async fn check_open_limits(
    tx: &mut Transaction<'_, Postgres>,
    new: &NewTicket<'_>,
) -> anyhow::Result<Option<TicketOpen>> {
    let guild = sql_id(new.guild_id);
    let Some(panel) = new.panel_id else { return Ok(None) };
    let limits: Option<(i32, i32, Option<i64>, i32)> = sqlx::query_as(
        "SELECT COALESCE(o.max_open_per_panel,0), COALESCE(o.max_open_per_category,0),
        COALESCE(p.open_category_id,s.open_category_id), COALESCE(o.cooldown_seconds,60)
        FROM ticket_panels p LEFT JOIN ticket_panel_operations o ON o.panel_id=p.id
        LEFT JOIN ticket_settings s ON s.guild_id=p.guild_id WHERE p.id=$1 AND p.guild_id=$2",
    )
    .bind(panel)
    .bind(guild)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((panel_max, category_max, category, cooldown)) = limits else {
        anyhow::bail!("ticket panel does not belong to guild");
    };
    if cooldown > 0 {
        let last: Option<chrono::DateTime<chrono::Utc>> =
            sqlx::query_scalar("SELECT max(opened_at) FROM tickets WHERE guild_id=$1 AND opener_id=$2")
                .bind(guild)
                .bind(sql_id(new.opener))
                .fetch_one(&mut **tx)
                .await?;
        if let Some(last) = last {
            let remaining = last + chrono::Duration::seconds(i64::from(cooldown)) - new.at;
            if remaining > chrono::Duration::zero() {
                return Ok(Some(TicketOpen::Cooldown {
                    seconds: remaining.num_seconds().max(1),
                }));
            }
        }
    }
    if panel_max > 0 {
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM tickets WHERE panel_id=$1 AND status='open'")
            .bind(panel)
            .fetch_one(&mut **tx)
            .await?;
        if count >= i64::from(panel_max) {
            return Ok(Some(TicketOpen::TooMany { open: count }));
        }
    }
    if category_max > 0
        && let Some(category) = category
    {
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM tickets t LEFT JOIN ticket_panels p ON p.id=t.panel_id
            LEFT JOIN ticket_settings s ON s.guild_id=t.guild_id WHERE t.guild_id=$1 AND t.status='open'
            AND COALESCE(p.open_category_id,s.open_category_id)=$2",
        )
        .bind(guild)
        .bind(category)
        .fetch_one(&mut **tx)
        .await?;
        if count >= i64::from(category_max) {
            return Ok(Some(TicketOpen::TooMany { open: count }));
        }
    }
    Ok(None)
}
