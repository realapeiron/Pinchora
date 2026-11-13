//! Recent dashboard activity visible to one current manager.
use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use chrono::{DateTime, Duration, Utc};
use pinchora_core::{admin::modules::Module, t};
use serde::Deserialize;
use serde_json::{Value, json};

pub fn router() -> Router<WebState> {
    Router::new().route("/", get(list)).route("/read", post(read))
}

use crate::{
    dashboard_access::route_area,
    error::{ApiError, ApiResult},
    extract::ManagedGuild,
    routes::community::tickets::names,
    state::WebState,
};

struct Item {
    at: DateTime<Utc>,
    summary: bool,
    value: Value,
}

fn allowed(managed: &ManagedGuild, path: &str) -> bool {
    route_area(path).is_some_and(|area| {
        managed
            .areas
            .as_ref()
            .is_none_or(|areas| areas.iter().any(|a| a == area))
    })
}

fn enabled(states: &[(Module, pinchora_core::admin::modules::ModuleRule)], module: Module) -> bool {
    states
        .iter()
        .find(|(kind, _)| *kind == module)
        .is_some_and(|(_, rule)| rule.enabled)
}

fn add(items: &mut Vec<Item>, kind: &str, id: String, at: DateTime<Utc>, link: &str, data: Value) {
    items.push(Item {
        at,
        summary: kind == "members",
        value: json!({"id": format!("{kind}:{id}"), "kind": kind, "at": at, "link": link, "data": data}),
    });
}

#[derive(sqlx::FromRow)]
struct AutoGroup {
    rule: i64,
    hour: i64,
    name: String,
    count: i64,
    channel: Option<i64>,
    at: DateTime<Utc>,
}

async fn safety_items(
    state: &WebState,
    managed: &ManagedGuild,
    states: &[(Module, pinchora_core::admin::modules::ModuleRule)],
    since: DateTime<Utc>,
    base: &str,
    items: &mut Vec<Item>,
) -> ApiResult<()> {
    let pool = state.app.db.pool();
    let guild = managed.id as i64;
    if allowed(managed, "automod") && enabled(states, Module::AutoMod) {
        let groups: Vec<AutoGroup> = sqlx::query_as(
            "SELECT COALESCE(h.rule_id,-h.id) AS rule, (EXTRACT(EPOCH FROM date_trunc('hour', h.created_at AT TIME ZONE 'UTC'))::bigint / 3600) AS hour, COALESCE(MAX(r.name),'') AS name, COUNT(*) AS count, CASE WHEN MIN(h.channel_id)=MAX(h.channel_id) THEN MIN(h.channel_id) END AS channel, MAX(h.created_at) AS at FROM automod_hits h LEFT JOIN automod_rules r ON r.id=h.rule_id AND r.guild_id=h.guild_id WHERE h.guild_id=$1 AND h.created_at >= $2 GROUP BY rule, hour ORDER BY at DESC LIMIT 20")
            .bind(guild).bind(since).fetch_all(pool).await?;
        for group in groups {
            add(
                items,
                "automod",
                format!("{}:{}", group.rule, group.hour),
                group.at,
                &format!("{base}/automod"),
                json!({"rule_name": group.name, "count": group.count, "channel_id": group.channel.map(|id| id.to_string())}),
            );
        }
    }
    if allowed(managed, "mod-parity") && enabled(states, Module::Moderation) {
        let reports: Vec<(i64, DateTime<Utc>)> = sqlx::query_as(
            "SELECT id, created_at FROM mod_reports WHERE guild_id=$1 AND status='pending' AND created_at >= $2 ORDER BY created_at DESC LIMIT 20")
            .bind(guild).bind(since).fetch_all(pool).await?;
        for (id, at) in reports {
            add(
                items,
                "report",
                id.to_string(),
                at,
                &format!("{base}/moderation-desk?queue=reports"),
                json!({}),
            );
        }
        let appeals: Vec<(i64, DateTime<Utc>)> = sqlx::query_as(
            "SELECT id, created_at FROM mod_appeals WHERE guild_id=$1 AND status='pending' AND created_at >= $2 ORDER BY created_at DESC LIMIT 20")
            .bind(guild).bind(since).fetch_all(pool).await?;
        for (id, at) in appeals {
            add(
                items,
                "appeal",
                id.to_string(),
                at,
                &format!("{base}/moderation-desk?queue=appeals"),
                json!({}),
            );
        }
    }
    Ok(())
}

async fn add_display_names(state: &WebState, items: &mut [Item]) {
    let people = names(
        state,
        items.iter().filter_map(|item| {
            let key = match item.value["kind"].as_str()? {
                "ticket" => "opener_id",
                "modmail" => "user_id",
                _ => return None,
            };
            item.value["data"][key].as_str()?.parse().ok()
        }),
    )
    .await;
    for item in items {
        let (id_key, name_key) = match item.value["kind"].as_str() {
            Some("ticket") => ("opener_id", "opener_name"),
            Some("modmail") => ("user_id", "user_name"),
            _ => continue,
        };
        if let Some(name) = item.value["data"][id_key]
            .as_str()
            .and_then(|id| id.parse::<u64>().ok())
            .and_then(|id| people.get(&id))
        {
            item.value["data"][name_key] = json!(name);
        }
    }
}

pub async fn list(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    let db = &state.app.db;
    let pool = db.pool();
    let guild = managed.id as i64;
    let since = Utc::now() - Duration::days(7);
    let base = format!("/g/{}", managed.id);
    let states = db.module_states(managed.id).await?;
    let mut items = Vec::new();

    if allowed(&managed, "tickets") && enabled(&states, Module::Tickets) {
        let rows: Vec<(i64, i32, i64, DateTime<Utc>)> = sqlx::query_as(
            "SELECT id, number, opener_id, opened_at FROM tickets WHERE guild_id=$1 AND opened_at >= $2 ORDER BY opened_at DESC LIMIT 20")
            .bind(guild).bind(since).fetch_all(pool).await?;
        for (id, number, opener, at) in rows {
            add(
                &mut items,
                "ticket",
                id.to_string(),
                at,
                &format!("{base}/tickets"),
                json!({"number": number, "opener_id": opener.to_string()}),
            );
        }
    }
    safety_items(&state, &managed, &states, since, &base, &mut items).await?;
    if allowed(&managed, "community-pack") && enabled(&states, Module::Modmail) {
        let rows: Vec<(i64, i64, DateTime<Utc>)> = sqlx::query_as(
            "SELECT id, user_id, opened_at FROM modmail_threads WHERE guild_id=$1 AND opened_at >= $2 ORDER BY opened_at DESC LIMIT 20")
            .bind(guild).bind(since).fetch_all(pool).await?;
        for (id, user, at) in rows {
            add(
                &mut items,
                "modmail",
                id.to_string(),
                at,
                &format!("{base}/community-engagement"),
                json!({"user_id": user.to_string()}),
            );
        }
    }
    if allowed(&managed, "stats") && enabled(&states, Module::Stats) {
        let rows: Vec<(DateTime<Utc>, i32)> = sqlx::query_as(
            "SELECT hour, joins FROM guild_activity_hourly WHERE guild_id=$1 AND hour >= $2 AND joins > 0 ORDER BY hour DESC LIMIT 24")
            .bind(guild).bind(Utc::now() - Duration::hours(24)).fetch_all(pool).await?;
        if let Some((at, _)) = rows.first() {
            let count: i64 = rows.iter().map(|(_, joins)| i64::from(*joins)).sum();
            add(
                &mut items,
                "members",
                at.timestamp().to_string(),
                *at,
                &format!("{base}/stats"),
                json!({"count": count}),
            );
        }
    }
    if allowed(&managed, "audit") {
        let rows: Vec<(i64, String, String, DateTime<Utc>)> = sqlx::query_as(
            "SELECT id, actor_name, action, created_at FROM dashboard_audit WHERE guild_id=$1 AND actor_id<>$2 AND created_at >= $3 ORDER BY created_at DESC LIMIT 20")
            .bind(guild).bind(managed.current.session.user_id as i64).bind(since).fetch_all(pool).await?;
        for (id, actor, action, at) in rows {
            add(
                &mut items,
                "audit",
                id.to_string(),
                at,
                &format!("{base}/audit"),
                json!({"actor_name": actor, "action": action}),
            );
        }
    }
    items.sort_by(|a, b| {
        b.at.cmp(&a.at)
            .then_with(|| a.value["id"].as_str().cmp(&b.value["id"].as_str()))
    });
    let read_at: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT read_at FROM dashboard_notification_reads WHERE guild_id=$1 AND manager_id=$2")
            .bind(guild)
            .bind(managed.current.session.user_id as i64)
            .fetch_optional(pool)
            .await?;
    let unread = items
        .iter()
        .filter(|item| !item.summary && read_at.is_none_or(|mark| item.at > mark))
        .count();
    items.truncate(20);
    add_display_names(&state, &mut items).await;
    Ok(Json(
        json!({"items": items.into_iter().map(|item| item.value).collect::<Vec<_>>(), "unread": unread, "read_at": read_at}),
    ))
}

#[derive(Deserialize)]
pub struct ReadBody {
    up_to: String,
}

pub async fn read(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Json(body): Json<ReadBody>,
) -> ApiResult<Json<Value>> {
    let lang = state.app.guild_lang(managed.id).await;
    let at = DateTime::parse_from_rfc3339(&body.up_to)
        .map_err(|_| ApiError::malformed(t!(lang, "notifications.bad_time")))?
        .with_timezone(&Utc);
    let read_at: DateTime<Utc> = sqlx::query_scalar(
        "INSERT INTO dashboard_notification_reads (guild_id, manager_id, read_at) VALUES ($1,$2,LEAST($3,now())) ON CONFLICT (guild_id,manager_id) DO UPDATE SET read_at=GREATEST(dashboard_notification_reads.read_at,LEAST(EXCLUDED.read_at,now())) RETURNING read_at")
        .bind(managed.id as i64).bind(managed.current.session.user_id as i64).bind(at).fetch_one(state.app.db.pool()).await?;
    Ok(Json(json!({"read_at": read_at})))
}
