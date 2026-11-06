//! `/autoroles`: roles members get on their own when they join, with a delay or an expiry if the server
//! wants one, and roles kept for members who leave and come back.

use pinchora_core::{
    i18n::Lang,
    roles_pack::{
        AutoKind, MAX_AUTO_ROLES, MAX_DELAY_SECS, MAX_EXPIRY_SECS, MIN_EXPIRY_SECS, STICKY_KEPT_DAYS, SpanError,
        format_span, parse_span_within,
    },
    t,
};
use pinchora_db::{AutoRoleRow, NewAutoRole};
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption};
use twilight_model::{
    channel::message::MessageFlags,
    guild::{Permissions, Role},
    http::interaction::InteractionResponseData,
    id::{Id, marker::RoleMarker},
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFooterBuilder};

use super::join::{start_sticky_snapshot, sticky_changed};
use crate::{Cx, roles, util::BRAND};

fn manage_guild() -> Permissions {
    Permissions::MANAGE_GUILD
}

/// The shortest delay that is not "at once": the ticker looks every half minute.
const MIN_DELAY_SECS: i64 = 30;

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "autoroles",
    desc = "Roles members get on their own",
    default_permissions = "manage_guild",
    contexts = "guild"
)]
#[allow(clippy::large_enum_variant)] // parsed once per interaction, never stored
pub enum AutoRolesCommand {
    #[command(name = "add")]
    Add(AutoAdd),
    #[command(name = "remove")]
    Remove(AutoRemove),
    #[command(name = "list")]
    List(AutoList),
    #[command(name = "sticky")]
    Sticky(AutoSticky),
}

#[derive(CommandOption, CreateOption, Clone, Copy)]
pub enum KindChoice {
    #[option(name = "Members who join", value = "join")]
    Join,
    #[option(name = "Bots that join", value = "bot")]
    Bot,
    #[option(name = "Members who pass the join gate", value = "after_verify")]
    AfterVerify,
}

impl KindChoice {
    fn kind(self) -> AutoKind {
        match self {
            Self::Join => AutoKind::Join,
            Self::Bot => AutoKind::Bot,
            Self::AfterVerify => AutoKind::AfterVerify,
        }
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "add", desc = "Give a role to everyone who joins (up to 10), or change one")]
pub struct AutoAdd {
    /// The role to give
    role: Role,
    /// Who gets it (default: members who join)
    kind: Option<KindChoice>,
    /// Wait this long after they join, like 10m or 1d (default: at once)
    #[command(max_length = 40)]
    delay: Option<String>,
    /// Take it away again after this long, like 7d (default: keep it)
    #[command(max_length = 40)]
    remove_after: Option<String>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "remove", desc = "Stop giving a role automatically")]
pub struct AutoRemove {
    /// The role to stop giving
    role: Id<RoleMarker>,
    /// Only this kind (default: every kind)
    kind: Option<KindChoice>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "list", desc = "Show the auto roles and whether sticky roles are on")]
pub struct AutoList;

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "sticky",
    desc = "Give members back their roles when they leave and come back"
)]
pub struct AutoSticky {
    /// Keep roles for members who leave
    enabled: bool,
}

/// Why a written time was refused, in `lang`; `what` names it ("delay", "remove_after").
fn span_problem(lang: Lang, what: &str, error: SpanError) -> String {
    match error {
        SpanError::Unreadable => t!(lang, "roles.auto.span-unreadable", what = what),
        SpanError::TooShort(secs) => t!(lang, "roles.auto.span-short", what = what, span = format_span(secs)),
        SpanError::TooLong(secs) => t!(lang, "roles.auto.span-long", what = what, span = format_span(secs)),
    }
}

/// A kind in words, in `lang`.
pub(crate) fn kind_name(lang: Lang, kind: &str) -> String {
    match AutoKind::parse(kind) {
        Some(AutoKind::Join) => t!(lang, "roles.auto.kind.join"),
        Some(AutoKind::Bot) => t!(lang, "roles.auto.kind.bot"),
        Some(AutoKind::AfterVerify) => t!(lang, "roles.auto.kind.after-verify"),
        None => kind.to_owned(),
    }
}

/// One auto role as a line: the role, who gets it, and its delay and expiry.
fn line(lang: Lang, row: &AutoRoleRow) -> String {
    let mut text = format!("<@&{}> · {}", row.role_id, kind_name(lang, &row.kind));
    if row.delay_secs > 0 {
        text.push_str(&format!(
            " · {}",
            t!(lang, "roles.auto.after", span = format_span(i64::from(row.delay_secs)))
        ));
    }
    if row.remove_after_secs > 0 {
        text.push_str(&format!(
            " · {}",
            t!(
                lang,
                "roles.auto.lasts",
                span = format_span(i64::from(row.remove_after_secs))
            )
        ));
    }
    if !row.enabled {
        text.push_str(&format!(" · {}", t!(lang, "roles.auto.paused")));
    }
    text
}

impl AutoRolesCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let lang = cx.lang();
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(lang, "roles.auto.guild-only")).await;
        };
        if !cx.can_manage_guild() {
            return cx.reply_ephemeral(t!(lang, "roles.auto.manage-needed")).await;
        }
        let db = &cx.app.db;
        match self {
            Self::Add(c) => {
                let kind = c.kind.map_or(AutoKind::Join, KindChoice::kind);
                let delay = match c.delay.as_deref() {
                    None => 0,
                    Some(text) => match parse_span_within(text, MIN_DELAY_SECS, MAX_DELAY_SECS) {
                        Ok(secs) => secs,
                        Err(error) => return cx.reply_ephemeral(span_problem(lang, "delay", error)).await,
                    },
                };
                let expiry = match c.remove_after.as_deref() {
                    None => 0,
                    Some(text) => match parse_span_within(text, MIN_EXPIRY_SECS, MAX_EXPIRY_SECS) {
                        Ok(secs) => secs,
                        Err(error) => return cx.reply_ephemeral(span_problem(lang, "remove_after", error)).await,
                    },
                };
                if let Some(why) = roles::unassignable(cx, guild, &c.role).await {
                    return cx.reply_ephemeral(why).await;
                }
                let auto = NewAutoRole {
                    role_id: c.role.id.get(),
                    kind: kind.key(),
                    delay_secs: delay as i32,
                    remove_after_secs: expiry as i32,
                    enabled: true,
                };
                if !db.save_auto_role(guild, &auto, MAX_AUTO_ROLES as i64).await? {
                    return cx
                        .reply_ephemeral(t!(lang, "roles.auto.full", max = MAX_AUTO_ROLES))
                        .await;
                }
                let count = db.auto_roles(guild).await?.len();
                let row = AutoRoleRow {
                    id: 0,
                    role_id: c.role.id.get() as i64,
                    kind: kind.key().to_owned(),
                    delay_secs: delay as i32,
                    remove_after_secs: expiry as i32,
                    enabled: true,
                };
                cx.reply_ephemeral(t!(
                    lang,
                    "roles.auto.added",
                    line = line(lang, &row),
                    count = count,
                    max = MAX_AUTO_ROLES
                ))
                .await
            }
            Self::Remove(c) => {
                let kind = c.kind.map(|k| k.kind().key());
                let removed = db.remove_auto_role(guild, c.role.get(), kind).await?;
                cx.reply_ephemeral(if removed > 0 {
                    t!(lang, "roles.auto.removed", role = format!("<@&{}>", c.role))
                } else {
                    t!(lang, "roles.auto.not-auto", role = format!("<@&{}>", c.role))
                })
                .await
            }
            Self::List(_) => {
                let rows = db.auto_roles(guild).await?;
                let sticky = db.sticky_enabled(guild).await?;
                let lines: Vec<String> = rows.iter().map(|r| line(lang, r)).collect();
                let body = if lines.is_empty() {
                    t!(lang, "roles.auto.none")
                } else {
                    lines.join("\n")
                };
                let embed = EmbedBuilder::new()
                    .color(BRAND)
                    .title(t!(lang, "roles.auto.title"))
                    .description(body)
                    .footer(EmbedFooterBuilder::new(t!(
                        lang,
                        "roles.auto.footer",
                        count = rows.len(),
                        max = MAX_AUTO_ROLES,
                        sticky = if sticky {
                            t!(lang, "roles.auto.sticky-on")
                        } else {
                            t!(lang, "roles.auto.sticky-off")
                        }
                    )))
                    .build();
                cx.reply(InteractionResponseData {
                    embeds: Some(vec![embed]),
                    flags: Some(MessageFlags::EPHEMERAL),
                    ..Default::default()
                })
                .await
            }
            Self::Sticky(c) => {
                db.set_sticky(guild, c.enabled).await?;
                sticky_changed(guild).await;
                if c.enabled {
                    start_sticky_snapshot(&cx.app, guild).await;
                }
                cx.reply_ephemeral(if c.enabled {
                    t!(lang, "roles.auto.sticky-enabled", days = STICKY_KEPT_DAYS)
                } else {
                    t!(lang, "roles.auto.sticky-disabled")
                })
                .await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_says_who_gets_the_role_and_when() {
        let row = AutoRoleRow {
            id: 1,
            role_id: 77,
            kind: "bot".to_owned(),
            delay_secs: 600,
            remove_after_secs: 86_400,
            enabled: false,
        };
        let text = line(Lang::En, &row);
        assert!(text.starts_with("<@&77> · Bots that join"), "{text}");
        assert!(
            text.contains("after 10m") && text.contains("lasts 1d") && text.contains("paused"),
            "{text}"
        );
        let plain = AutoRoleRow {
            delay_secs: 0,
            remove_after_secs: 0,
            enabled: true,
            kind: "join".to_owned(),
            ..row
        };
        assert_eq!(line(Lang::En, &plain), "<@&77> · Members who join");
        assert_eq!(kind_name(Lang::En, "zzz"), "zzz");
    }

    #[test]
    fn a_bad_time_is_explained() {
        let text = span_problem(Lang::En, "delay", SpanError::Unreadable);
        assert!(text.contains("delay") && text.contains("10m"), "{text}");
        let long = span_problem(Lang::En, "remove_after", SpanError::TooLong(MAX_EXPIRY_SECS));
        assert!(long.contains("365d"), "{long}");
    }
}
