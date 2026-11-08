use crate::{App, custom};
use pinchora_core::{
    modules::Module,
    workflows::{self, Condition, Event, Plan, Scope, Trigger},
};
use pinchora_db::WorkflowRow;
use std::time::Duration;
use twilight_model::id::Id;

pub fn dry_run(def: &workflows::Definition, event: &Event, now: chrono::DateTime<chrono::Utc>) -> Plan {
    workflows::plan_with_preview(def, event, now, super::messages::preview)
}
pub(super) fn number(id: &str) -> anyhow::Result<u64> {
    let n = id.parse::<u64>()?;
    anyhow::ensure!(n > 0, "missing_context");
    Ok(n)
}
fn cooldown(guild: u64, id: i64, scope: Scope, user: &str, index: usize) -> String {
    format!(
        "wf:cool:{guild}:{id}:{index}:{}",
        if scope == Scope::Server { "0" } else { user }
    )
}
pub async fn run(app: &App, row: &WorkflowRow, mut event: Event) -> anyhow::Result<Plan> {
    let guild = row.guild_id as u64;
    let def = row.definition()?;
    anyhow::ensure!(event.depth == 0, "workflow_depth");
    anyhow::ensure!(
        app.guild_modules(guild).await?.rule(Module::Workflows).enabled
            && super::available(app, guild, &def.trigger).await,
        "module_disabled"
    );
    anyhow::ensure!(
        !matches!(def.trigger, Trigger::MessageKeyword { .. }) || app.message_content,
        "message_content_required"
    );
    def.validate().map_err(anyhow::Error::msg)?;
    let run = app
        .db
        .claim_workflow_run(guild, row.id, &event)
        .await?
        .ok_or_else(|| anyhow::anyhow!("budget"))?;
    let result = async {
        load_context(app, row, &def, &mut event).await?;
        let mut plan = dry_run(&def, &event, app.now());
        if plan.outcome == "ready" {
            let cooldowns: Vec<_> = def
                .conditions
                .iter()
                .enumerate()
                .filter_map(|(index, condition)| {
                    if let Condition::Cooldown { scope, seconds } = condition {
                        Some((
                            index,
                            (
                                cooldown(guild, row.id, *scope, &event.user_id, index),
                                Duration::from_secs((*seconds).into()),
                            ),
                        ))
                    } else {
                        None
                    }
                })
                .collect();
            let keys: Vec<_> = cooldowns.iter().map(|(_, key)| key.clone()).collect();
            for blocked in app.cache.claim_workflow_cooldowns(&keys).await? {
                event.cooldowns.push(cooldowns[blocked].0);
            }
            plan = dry_run(&def, &event, app.now());
        }
        execute_plan(app, row, run, &mut event, &mut plan).await?;
        Ok::<_, anyhow::Error>(plan)
    }
    .await;
    match result {
        Ok(plan) => {
            app.db
                .finish_workflow_run(guild, run, &plan.outcome, &serde_json::to_value(&plan.steps)?)
                .await?;
            Ok(plan)
        }
        Err(error) => {
            app.db
                .finish_workflow_run(
                    guild,
                    run,
                    "failed",
                    &serde_json::json!([{"phase":"context","index":0,"status":"failed","detail":{}}]),
                )
                .await?;
            Err(error)
        }
    }
}

async fn load_context(
    app: &App,
    row: &WorkflowRow,
    def: &workflows::Definition,
    event: &mut Event,
) -> anyhow::Result<()> {
    let guild = row.guild_id as u64;
    let user = event.user_id.parse::<u64>().unwrap_or(0);
    if user > 0 && !matches!(def.trigger, Trigger::MemberLeave {}) {
        let member = app
            .http
            .guild_member(Id::new(guild), Id::new(user))
            .await?
            .model()
            .await?;
        anyhow::ensure!(!member.user.bot, "bot_event");
        event.roles = member.roles.iter().map(ToString::to_string).collect();
    }
    if user > 0 {
        if let Some(rank) = app.db.member_rank(guild, user).await? {
            event.level = app.leveling_parity(guild).await?.level_of(rank.xp);
        }
        let created = ((user >> 22) as i64 + 1420070400000) / 1000;
        event.account_age_days = ((app.now().timestamp() - created).max(0) / 86400) as u32;
    }
    event.roll = custom::pick(1000000) as u32;
    event.variables = app
        .db
        .workflow_vars(guild, user)
        .await?
        .into_iter()
        .map(|(scope, key, value)| (format!("{scope}:{key}"), value))
        .collect();
    Ok(())
}

async fn execute_plan(
    app: &App,
    row: &WorkflowRow,
    run: i64,
    event: &mut Event,
    plan: &mut Plan,
) -> anyhow::Result<()> {
    let guild = row.guild_id as u64;
    let mut writes = 0;
    for (index, action) in plan.actions.clone().iter().enumerate() {
        writes += action.writes();
        anyhow::ensure!(writes <= workflows::MAX_WRITES, "writes");
        // Persist before each write so interrupted runs show the last attempted step, never replay it.
        if let Some(step) = plan.steps.iter_mut().find(|s| s.phase == "action" && s.index == index) {
            step.status = "running".into();
        }
        app.db
            .finish_workflow_run(guild, run, "running", &serde_json::to_value(&plan.steps)?)
            .await?;
        let result = super::actions::execute(app, guild, event, action).await;
        if let Some(step) = plan.steps.iter_mut().find(|s| s.phase == "action" && s.index == index) {
            step.status = if result.is_ok() { "done" } else { "failed" }.into();
        }
        if let Err(error) = result {
            tracing::warn!(guild,workflow=row.id,%error,"workflow action failed");
            plan.outcome = "failed".into();
            break;
        }
    }
    if plan.outcome == "ready" {
        plan.outcome = "completed".into();
    }
    for s in &mut plan.steps {
        if s.status == "would_run" {
            s.status = "skipped".into();
        }
    }
    Ok(())
}
