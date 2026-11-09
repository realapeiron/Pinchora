use super::*;

pub(super) async fn refund_game_stakes(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: i64,
    hosted: Vec<(i64,)>,
) -> anyhow::Result<()> {
    let held: Vec<(i64, i64, Option<i64>, i64)> = sqlx::query_as(
        "SELECT id, host_id, guest_id, bet FROM game_sessions
             WHERE status = 'active' AND bet > 0 AND (host_id = $1 OR guest_id = $1)
             ORDER BY id FOR UPDATE",
    )
    .bind(id)
    .fetch_all(&mut **tx)
    .await?;
    for (_, host, guest, bet) in held {
        let other = if host == id { guest } else { Some(host) };
        if let Some(other) = other.filter(|o| *o != id) {
            sqlx::query("UPDATE users SET credits = credits + $2 WHERE id = $1")
                .bind(other)
                .bind(bet)
                .execute(&mut **tx)
                .await?;
            crate::economy::legacy::ledger(tx, other, bet, "duel_refund").await?;
        }
    }
    for (session,) in hosted {
        let racers: Vec<crate::PartyMember> = crate::games::party::members(tx, session)
            .await?
            .into_iter()
            .filter(|racer| racer.user_id != id)
            .collect();
        crate::games::party::refund_all(tx, session, &racers).await?;
    }
    Ok(())
}

pub(super) async fn unlink_shared_records(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: i64,
    now: DateTime<Utc>,
    today: NaiveDate,
) -> anyhow::Result<()> {
    // Today's limits stay; the days before go.
    let day = now.timestamp().div_euclid(86_400);
    for (table, column, day_column) in DAILY_LIMITS {
        let sql = format!("DELETE FROM {table} WHERE {column} = $1 AND {day_column} < $2");
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .bind(day as i32)
            .execute(&mut **tx)
            .await?;
    }
    for (table, column, name) in ANONYMISED {
        let sql = format!("UPDATE {table} SET {column} = 0, {name} = $2 WHERE {column} = $1");
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .bind(DELETED_NAME)
            .execute(&mut **tx)
            .await?;
    }

    for (table, column) in ZEROED_COLUMNS {
        let sql = format!("UPDATE {table} SET {column} = 0 WHERE {column} = $1");
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .execute(&mut **tx)
            .await?;
    }

    // A guest's paid visits of the day stay counted when the host they went to is deleted, under
    // a number that is nobody's.
    sqlx::query("UPDATE visits SET host_id = -nextval('deleted_ids') WHERE host_id = $1 AND day >= $2")
        .bind(id)
        .bind(today)
        .execute(&mut **tx)
        .await?;
    // The likes they gave come off the houses they went to.
    sqlx::query(
        "UPDATE houses h SET likes = GREATEST(h.likes - l.n, 0)
             FROM (SELECT host_id, count(*)::INTEGER AS n FROM house_likes WHERE liker_id = $1 GROUP BY host_id) l
             WHERE h.user_id = l.host_id",
    )
    .bind(id)
    .execute(&mut **tx)
    .await?;

    for (table, column, edit) in ID_LISTS {
        let value = match edit {
            ListEdit::Blank => format!("array_replace({column}, $1, 0)"),
            ListEdit::Taken => format!("array_replace({column}, $1, -1)"),
            ListEdit::Remove => format!("array_remove({column}, $1)"),
        };
        let sql = format!("UPDATE {table} SET {column} = {value} WHERE $1 = ANY({column})");
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .execute(&mut **tx)
            .await?;
    }
    for (table, column) in CLEARED_COLUMNS {
        let sql = format!("UPDATE {table} SET {column} = NULL WHERE {column} = $1");
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .execute(&mut **tx)
            .await?;
    }

    Ok(())
}

pub(super) async fn delete_registered_rows(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user: u64,
    id: i64,
) -> anyhow::Result<Deleted> {
    let mut deleted = Deleted::default();
    let mut touched: Vec<&str> = Vec::new();
    let affected_stars: Vec<(i64, i64)> =
        sqlx::query_as("SELECT starboard_id,message_id FROM starboard_stars WHERE user_id=$1")
            .bind(id)
            .fetch_all(&mut **tx)
            .await?;
    sqlx::query("INSERT INTO starboard_copy_cleanup(channel_id,copy_id) SELECT b.channel_id,p.board_message_id FROM starboard_posts p JOIN starboards b ON b.id=p.starboard_id WHERE p.author_id=$1 AND p.board_message_id>0 ON CONFLICT DO NOTHING")
        .bind(id).execute(&mut **tx).await?;
    sqlx::query("DELETE FROM roblox_cache WHERE key IN (SELECT 'groups:' || roblox_id::text FROM roblox_links WHERE user_id=$1)")
        .bind(sql_id(user)).execute(&mut **tx).await?;
    // Remember guest sessions before the registry removes their seat and request receipts.
    let joined_games: Vec<i64> = sqlx::query_scalar("SELECT game_id FROM reef_game_players WHERE user_id=$1")
        .bind(id)
        .fetch_all(&mut **tx)
        .await?;
    for (table, column) in USER_COLUMNS {
        // Table and column names are the constants above.
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!("DELETE FROM {table} WHERE {column} = $1")))
            .bind(id)
            .execute(&mut **tx)
            .await?
            .rows_affected();
        if rows > 0 {
            deleted.rows += rows;
            if !touched.contains(table) {
                touched.push(table);
            }
        }
    }
    for (board, message) in affected_stars {
        sqlx::query("UPDATE starboard_posts SET star_count=(SELECT COALESCE(SUM(weight),0)::INT FROM starboard_stars WHERE starboard_id=$1 AND message_id=$2),sync_due=now(),sync_version=sync_version+1 WHERE starboard_id=$1 AND message_id=$2")
            .bind(board).bind(message).execute(&mut **tx).await?;
    }
    // A removed seat must not leave a live game that can pay or be reused after deletion.
    let games = sqlx::query("DELETE FROM reef_games WHERE id=ANY($1)")
        .bind(&joined_games)
        .execute(&mut **tx)
        .await?
        .rows_affected();
    deleted.rows += games;
    if games > 0 && !touched.contains(&"reef_games") {
        touched.push("reef_games");
    }
    deleted.tables = touched.len();
    Ok(deleted)
}
