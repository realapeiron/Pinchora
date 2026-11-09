use crate::Cx;
use pinchora_core::admin::workflows::Trigger;
use pinchora_core::t;
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::guild::Permissions;
fn manage() -> Permissions {
    Permissions::MANAGE_GUILD
}
#[derive(CommandModel, CreateCommand)]
#[command(
    name = "workflow",
    desc = "Manage server workflows",
    default_permissions = "manage",
    contexts = "guild"
)]
pub enum Workflow {
    #[command(name = "list")]
    List(List),
    #[command(name = "toggle")]
    Toggle(Toggle),
    #[command(name = "runs")]
    Runs(Runs),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "list", desc = "List")]
pub struct List {}
#[derive(CommandModel, CreateCommand)]
#[command(name = "toggle", desc = "Enable or disable")]
pub struct Toggle {
    /// Name
    #[command(max_length = 40)]
    name: String,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "runs", desc = "Recent runs")]
pub struct Runs {
    /// Name
    #[command(max_length = 40)]
    name: String,
}
impl Workflow {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        if !cx.can_manage_guild() {
            return cx.reply_ephemeral(t!(cx.lang(), "config.needs-manage-server")).await;
        }
        let guild = cx.guild_id().ok_or_else(|| anyhow::anyhow!("guild"))?;
        let rows = cx.app.db.workflows(guild).await?;
        let body = match self {
            Self::List(_) => rows
                .iter()
                .map(|r| {
                    format!(
                        "{} · {}",
                        r.name,
                        t!(cx.lang(), if r.enabled { "workflows.on" } else { "workflows.off" })
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
            Self::Toggle(c) => {
                let Some(row) = rows.iter().find(|r| r.name == c.name) else {
                    return cx.reply_ephemeral(t!(cx.lang(), "workflows.unavailable")).await;
                };
                let def = row.definition()?;
                if !row.enabled
                    && (!crate::admin::workflows::available(&cx.app, guild, &def.trigger).await
                        || matches!(def.trigger, Trigger::MessageKeyword { .. }) && !cx.app.message_content)
                {
                    return cx.reply_ephemeral(t!(cx.lang(), "workflows.unavailable")).await;
                }
                let enabled = cx.app.db.toggle_workflow(guild, &c.name).await?.unwrap_or(false);
                let before = serde_json::json!({"enabled":row.enabled});
                let after = serde_json::json!({"enabled":enabled});
                cx.app
                    .db
                    .audit(&pinchora_db::NewAudit {
                        guild,
                        actor: cx.author()?.id.get(),
                        actor_name: &cx.author_display_name(),
                        source: pinchora_db::AuditSource::Discord,
                        action: "workflow.toggle",
                        target: Some(&c.name),
                        before: Some(&before.to_string()),
                        after: Some(&after.to_string()),
                    })
                    .await?;
                t!(cx.lang(), if enabled { "workflows.on" } else { "workflows.off" })
            }
            Self::Runs(c) => {
                let Some(row) = rows.iter().find(|r| r.name == c.name) else {
                    return cx.reply_ephemeral(t!(cx.lang(), "workflows.unavailable")).await;
                };
                cx.app
                    .db
                    .workflow_runs(guild, row.id)
                    .await?
                    .iter()
                    .take(10)
                    .map(|r| {
                        format!(
                            "{} · {}",
                            r.started_at.format("%m-%d %H:%M"),
                            t!(cx.lang(), &format!("workflows.{}", r.outcome))
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        };
        cx.reply_ephemeral(if body.is_empty() {
            t!(cx.lang(), "workflows.empty")
        } else {
            pinchora_core::community::tags::truncate(&body, 1900)
        })
        .await
    }
}
