//! `/customcommand`: write, change and remove the server's custom commands from Discord. The text-in,
//! text-out case lives here (one form); embeds, actions, restrictions and everything else are on the
//! dashboard. Both go through the same checks (`pinchora_core::tags`) and the same writes.

use std::{str::Split, sync::OnceLock};

use pinchora_core::{
    i18n::Lang,
    t,
    tags::{
        COMMAND_COOLDOWN_MAX, CommandDef, DESCRIPTION_MAX, MAX_COMMANDS, RESPONSE_MAX, ReplyMode, check_name,
        tidy_name, truncate,
    },
};
use pinchora_db::{AuditSource, CommandRow, NewAudit, TagWrite};
use serde_json::Value;
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    channel::message::{
        Component, Embed,
        component::{ActionRow, ButtonStyle, Label, TextInput, TextInputStyle},
    },
    guild::Permissions,
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder, EmbedFooterBuilder};

use crate::{Cx, commands::modules::announce_guild_change, custom::def_diff, server_events::button, util::BRAND};

/// Commands per page of `/customcommand list`.
const PAGE: usize = 10;

const RESPONSE_FIELD: &str = "response";
const DESCRIPTION_FIELD: &str = "description";
const COOLDOWN_FIELD: &str = "cooldown";

fn manage_guild() -> Permissions {
    Permissions::MANAGE_GUILD
}

/// The names the bot's own commands use, which a custom command cannot take.
pub fn reserved_names() -> &'static [String] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    NAMES.get_or_init(|| super::definitions().into_iter().map(|c| c.name).collect())
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "customcommand",
    desc = "Write the custom commands members run with /tag",
    default_permissions = "manage_guild",
    contexts = "guild"
)]
#[allow(clippy::large_enum_variant)] // parsed once per interaction, never stored
pub enum CustomCommand {
    #[command(name = "create")]
    Create(CustomCreate),
    #[command(name = "edit")]
    Edit(CustomEdit),
    #[command(name = "delete")]
    Delete(CustomDelete),
    #[command(name = "list")]
    List(CustomList),
    #[command(name = "info")]
    Info(CustomInfo),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "create", desc = "Write a new custom command (a form asks for its reply)")]
pub struct CustomCreate {
    /// What members type after /tag: letters, numbers, - and _
    #[command(min_length = 1, max_length = 32)]
    name: String,
    /// A line shown next to the name in suggestions
    #[command(max_length = 100)]
    description: Option<String>,
    /// Answer privately, so only the person who ran it sees it
    private: Option<bool>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "edit", desc = "Change a custom command's reply, description or cooldown")]
pub struct CustomEdit {
    /// The command to change (start typing its name)
    #[command(autocomplete = true, max_length = 32)]
    name: String,
    /// Answer privately (true) or in the channel (false); left alone when not given
    private: Option<bool>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "delete", desc = "Remove a custom command")]
pub struct CustomDelete {
    /// The command to remove (start typing its name)
    #[command(autocomplete = true, max_length = 32)]
    name: String,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "list", desc = "Every custom command of this server")]
pub struct CustomList;

#[derive(CommandModel, CreateCommand)]
#[command(name = "info", desc = "How one custom command is set up")]
pub struct CustomInfo {
    /// The command to look at (start typing its name)
    #[command(autocomplete = true, max_length = 32)]
    name: String,
}

fn needs_manage(lang: Lang) -> String {
    t!(lang, "customcommand.needs-manage")
}

impl CustomCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "customcommand.guild-only")).await;
        };
        if !cx.can_manage_guild() {
            return cx.reply_ephemeral(needs_manage(cx.lang())).await;
        }
        match self {
            Self::Create(c) => create(cx, guild, c).await,
            Self::Edit(c) => edit(cx, guild, c).await,
            Self::Delete(c) => delete(cx, guild, c).await,
            Self::List(_) => show_list(cx, guild).await,
            Self::Info(c) => info(cx, guild, c).await,
        }
    }
}

// /Forms

/// A text box inside a form, with the label and the line under it.
#[allow(deprecated)] // TextInput::label: the Label around it carries the label now
fn field(
    id: &str,
    label: &str,
    hint: String,
    style: TextInputStyle,
    max: usize,
    value: Option<String>,
    required: bool,
) -> Component {
    Component::Label(Label {
        component: Box::new(Component::TextInput(TextInput {
            custom_id: id.to_owned(),
            id: None,
            label: None,
            max_length: Some(max as u16),
            min_length: required.then_some(1),
            placeholder: None,
            required: Some(required),
            style,
            value,
        })),
        description: Some(hint),
        id: None,
        label: label.to_owned(),
    })
}

fn form(
    lang: Lang,
    reply: Option<String>,
    description: Option<String>,
    cooldown: Option<i64>,
    replies: usize,
) -> Vec<Component> {
    let hint = if replies > 1 {
        t!(lang, "customcommand.hint-many", count = replies)
    } else {
        t!(lang, "customcommand.hint", max = RESPONSE_MAX)
    };
    vec![
        field(
            RESPONSE_FIELD,
            &t!(lang, "customcommand.reply-label"),
            hint,
            TextInputStyle::Paragraph,
            RESPONSE_MAX,
            reply,
            true,
        ),
        field(
            DESCRIPTION_FIELD,
            &t!(lang, "customcommand.description-label"),
            t!(lang, "customcommand.description-hint", max = DESCRIPTION_MAX),
            TextInputStyle::Short,
            DESCRIPTION_MAX,
            description.filter(|d| !d.is_empty()),
            false,
        ),
        field(
            COOLDOWN_FIELD,
            &t!(lang, "customcommand.cooldown-label"),
            t!(lang, "customcommand.cooldown-hint", max = COMMAND_COOLDOWN_MAX),
            TextInputStyle::Short,
            4,
            cooldown.filter(|c| *c > 0).map(|c| c.to_string()),
            false,
        ),
    ]
}

async fn create(cx: &Cx, guild: u64, c: CustomCreate) -> anyhow::Result<()> {
    let name = match check_name(&c.name, reserved_names()) {
        Ok(name) => name,
        Err(error) => return cx.reply_ephemeral(error.text(cx.lang())).await,
    };
    let set = cx.app.tag_set(guild).await?;
    if set.commands.iter().any(|x| x.def.name == name) {
        return cx
            .reply_ephemeral(t!(cx.lang(), "customcommand.exists", name = name))
            .await;
    }
    if set.commands.len() >= MAX_COMMANDS {
        return cx.reply_ephemeral(full(cx.lang())).await;
    }
    let private = u8::from(c.private.unwrap_or(false));
    cx.modal(
        format!("cm:c:{name}:{private}"),
        &t!(cx.lang(), "customcommand.new-title", name = name),
        form(cx.lang(), None, c.description, None, 1),
    )
    .await
}

async fn edit(cx: &Cx, guild: u64, c: CustomEdit) -> anyhow::Result<()> {
    let Some(row) = find(cx, guild, &c.name).await? else {
        return gone(cx, &c.name).await;
    };
    let keep = match c.private {
        Some(true) => "1",
        Some(false) => "0",
        None => "k",
    };
    let def = &row.def;
    cx.modal(
        format!("cm:e:{}:{keep}", row.id),
        &t!(cx.lang(), "customcommand.edit-title", name = def.name),
        form(
            cx.lang(),
            def.responses.first().cloned(),
            Some(def.description.clone()),
            Some(def.cooldown_secs),
            def.responses.len(),
        ),
    )
    .await
}

fn full(lang: Lang) -> String {
    t!(lang, "customcommand.full", max = MAX_COMMANDS)
}

/// A command of the server by its name, as typed or picked.
async fn find(cx: &Cx, guild: u64, name: &str) -> anyhow::Result<Option<CommandRow>> {
    let name = tidy_name(name);
    Ok(cx
        .app
        .tag_set(guild)
        .await?
        .commands
        .iter()
        .find(|c| c.def.name == name)
        .cloned())
}

async fn gone(cx: &Cx, name: &str) -> anyhow::Result<()> {
    let shown = truncate(&tidy_name(name), 40).replace('`', "'");
    cx.reply_ephemeral(t!(cx.lang(), "customcommand.none", name = shown))
        .await
}

// /Form submitted

fn text<'a>(fields: &'a [(String, String)], id: &str) -> &'a str {
    fields
        .iter()
        .find(|(field, _)| field == id)
        .map_or("", |(_, value)| value.as_str())
}

/// The cooldown box: empty is none, anything else must be a whole number of seconds.
fn cooldown_of(lang: Lang, raw: &str) -> Result<i64, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(0);
    }
    raw.parse::<i64>()
        .ok()
        .filter(|n| (0..=COMMAND_COOLDOWN_MAX).contains(n))
        .ok_or_else(|| t!(lang, "customcommand.cooldown-bad", max = COMMAND_COOLDOWN_MAX))
}

/// A form of `/customcommand create` or `edit`, submitted: `cm:c:<name>:<private>` or `cm:e:<id>:<private>`.
pub(crate) async fn on_modal(cx: &Cx, mut parts: Split<'_, char>, fields: &[(String, String)]) -> anyhow::Result<()> {
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral(t!(cx.lang(), "customcommand.guild-only")).await;
    };
    // The form can be opened by a manager and submitted later, after their permission changed.
    if !cx.can_manage_guild() {
        return cx.reply_ephemeral(needs_manage(cx.lang())).await;
    }
    let (kind, target, private) = (parts.next(), parts.next().unwrap_or(""), parts.next().unwrap_or("0"));
    match kind {
        Some("c") => created(cx, guild, target, private == "1", fields).await,
        Some("e") => edited(cx, guild, target, private, fields).await,
        _ => cx.reply_ephemeral(t!(cx.lang(), "customcommand.form-expired")).await,
    }
}

async fn created(cx: &Cx, guild: u64, name: &str, private: bool, fields: &[(String, String)]) -> anyhow::Result<()> {
    let cooldown = match cooldown_of(cx.lang(), text(fields, COOLDOWN_FIELD)) {
        Ok(seconds) => seconds,
        Err(message) => return cx.reply_ephemeral(message).await,
    };
    let def = CommandDef {
        name: name.to_owned(),
        description: text(fields, DESCRIPTION_FIELD).to_owned(),
        responses: vec![text(fields, RESPONSE_FIELD).to_owned()],
        embed: None,
        reply_mode: if private {
            ReplyMode::Ephemeral
        } else {
            ReplyMode::Public
        },
        actions: Vec::new(),
        allow_roles: Vec::new(),
        deny_roles: Vec::new(),
        allow_channels: Vec::new(),
        deny_channels: Vec::new(),
        cooldown_secs: cooldown,
        enabled: true,
        allow_mentions: false,
    };
    let def = match def.clean(reserved_names()) {
        Ok(def) => def,
        Err(errors) => return cx.reply_ephemeral(errors[0].text(cx.lang())).await,
    };
    let author = cx.author()?.id.get();
    match cx.app.db.add_custom_command(guild, &def, author).await? {
        TagWrite::Saved(row) => {
            announce_guild_change(&cx.app, guild).await;
            let after = serde_json::to_value(&row.def)?;
            audit(cx, guild, "command.create", &row.def.name, Value::Null, after).await;
            cx.reply_ephemeral(t!(cx.lang(), "customcommand.created", name = row.def.name))
                .await
        }
        TagWrite::NameTaken => {
            cx.reply_ephemeral(t!(cx.lang(), "customcommand.exists", name = def.name))
                .await
        }
        TagWrite::Full => cx.reply_ephemeral(full(cx.lang())).await,
        TagWrite::Gone => unreachable!("an insert is never gone"),
    }
}

async fn edited(cx: &Cx, guild: u64, id: &str, private: &str, fields: &[(String, String)]) -> anyhow::Result<()> {
    let Some(row) = ({
        let id: i64 = id.parse().unwrap_or(0);
        cx.app.db.custom_command(guild, id).await?
    }) else {
        return cx
            .reply_ephemeral(t!(cx.lang(), "customcommand.removed-while-open"))
            .await;
    };
    let cooldown = match cooldown_of(cx.lang(), text(fields, COOLDOWN_FIELD)) {
        Ok(seconds) => seconds,
        Err(message) => return cx.reply_ephemeral(message).await,
    };
    let mut def = row.def.clone();
    let reply = text(fields, RESPONSE_FIELD).to_owned();
    match def.responses.first_mut() {
        Some(first) => *first = reply,
        None => def.responses.push(reply),
    }
    def.description = text(fields, DESCRIPTION_FIELD).to_owned();
    def.cooldown_secs = cooldown;
    match private {
        "1" => def.reply_mode = ReplyMode::Ephemeral,
        "0" => def.reply_mode = ReplyMode::Public,
        _ => {}
    }
    // The name does not change, so it is not held against the bot's commands again.
    let def = match def.clean(&[]) {
        Ok(def) => def,
        Err(errors) => return cx.reply_ephemeral(errors[0].text(cx.lang())).await,
    };
    match cx.app.db.replace_custom_command(guild, row.id, &def).await? {
        TagWrite::Saved(saved) => {
            announce_guild_change(&cx.app, guild).await;
            let (before, after) = def_diff(&serde_json::to_value(&row.def)?, &serde_json::to_value(&saved.def)?);
            if after.as_object().is_some_and(|a| !a.is_empty()) {
                audit(cx, guild, "command.update", &saved.def.name, before, after).await;
            }
            cx.reply_ephemeral(t!(cx.lang(), "customcommand.saved", name = saved.def.name))
                .await
        }
        _ => {
            cx.reply_ephemeral(t!(cx.lang(), "customcommand.removed-while-open"))
                .await
        }
    }
}

// /Delete, list, info

async fn delete(cx: &Cx, guild: u64, c: CustomDelete) -> anyhow::Result<()> {
    let Some(row) = find(cx, guild, &c.name).await? else {
        return gone(cx, &c.name).await;
    };
    match cx.app.db.delete_custom_command(guild, row.id).await? {
        Some(removed) => {
            announce_guild_change(&cx.app, guild).await;
            audit(
                cx,
                guild,
                "command.delete",
                &removed.def.name,
                serde_json::to_value(&removed.def)?,
                Value::Null,
            )
            .await;
            cx.reply_ephemeral(t!(cx.lang(), "customcommand.deleted", name = removed.def.name))
                .await
        }
        None => gone(cx, &c.name).await,
    }
}

/// One page of the list, and the buttons that turn it.
fn page_view(lang: Lang, commands: &[CommandRow], page: usize) -> (Embed, Vec<Component>) {
    let pages = commands.len().div_ceil(PAGE).max(1);
    let page = page.min(pages - 1);
    let lines: Vec<String> = commands
        .iter()
        .skip(page * PAGE)
        .take(PAGE)
        .map(|c| {
            let mut line = format!("`/tag {}`", c.def.name);
            if !c.def.description.is_empty() {
                line.push_str(&format!(" · {}", c.def.description));
            }
            line.push_str(&t!(lang, "customcommand.list-used", count = c.uses));
            if !c.def.enabled {
                line.push_str(&t!(lang, "customcommand.list-off"));
            }
            line
        })
        .collect();
    let description = if lines.is_empty() {
        t!(lang, "customcommand.list-empty")
    } else {
        lines.join("\n")
    };
    let embed = EmbedBuilder::new()
        .title(t!(
            lang,
            "customcommand.list-title",
            count = commands.len(),
            max = MAX_COMMANDS
        ))
        .description(description)
        .color(BRAND)
        .footer(EmbedFooterBuilder::new(t!(
            lang,
            "customcommand.list-page",
            page = page + 1,
            pages = pages
        )))
        .build();
    let mut components = Vec::new();
    if pages > 1 {
        components.push(Component::ActionRow(ActionRow {
            id: None,
            components: vec![
                button(
                    format!("tg:l:{}", page.saturating_sub(1)),
                    &t!(lang, "customcommand.previous"),
                    None,
                    ButtonStyle::Secondary,
                    page == 0,
                ),
                button(
                    format!("tg:l:{}", page + 1),
                    &t!(lang, "customcommand.next"),
                    None,
                    ButtonStyle::Secondary,
                    page + 1 >= pages,
                ),
            ],
        }));
    }
    (embed, components)
}

async fn show_list(cx: &Cx, guild: u64) -> anyhow::Result<()> {
    let set = cx.app.tag_set(guild).await?;
    let (embed, components) = page_view(cx.lang(), &set.commands, 0);
    cx.reply(ephemeral(embed, components)).await
}

fn ephemeral(embed: Embed, components: Vec<Component>) -> InteractionResponseData {
    InteractionResponseData {
        embeds: Some(vec![embed]),
        components: Some(components),
        flags: Some(twilight_model::channel::message::MessageFlags::EPHEMERAL),
        ..Default::default()
    }
}

/// A page button of the list, clicked: `tg:l:<page>`.
pub(crate) async fn on_page(cx: &Cx, mut parts: Split<'_, char>) -> anyhow::Result<()> {
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral(t!(cx.lang(), "customcommand.guild-only")).await;
    };
    if parts.next() != Some("l") {
        return cx.reply_ephemeral(t!(cx.lang(), "social.button-expired")).await;
    }
    if !cx.can_manage_guild() {
        return cx.reply_ephemeral(needs_manage(cx.lang())).await;
    }
    let page: usize = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let set = cx.app.tag_set(guild).await?;
    let (embed, components) = page_view(cx.lang(), &set.commands, page);
    cx.update(InteractionResponseData {
        embeds: Some(vec![embed]),
        components: Some(components),
        ..Default::default()
    })
    .await
}

async fn info(cx: &Cx, guild: u64, c: CustomInfo) -> anyhow::Result<()> {
    let Some(row) = find(cx, guild, &c.name).await? else {
        return gone(cx, &c.name).await;
    };
    let d = &row.def;
    let lang = cx.lang();
    let mut embed = EmbedBuilder::new()
        .title(format!("/tag {}", d.name))
        .color(BRAND)
        .field(
            EmbedFieldBuilder::new(
                t!(lang, "customcommand.state"),
                t!(
                    lang,
                    if d.enabled {
                        "customcommand.on"
                    } else {
                        "customcommand.off"
                    }
                ),
            )
            .inline(),
        )
        .field(EmbedFieldBuilder::new(t!(lang, "customcommand.used"), format!("{}×", row.uses)).inline())
        .field(
            EmbedFieldBuilder::new(
                t!(lang, "customcommand.answers"),
                t!(
                    lang,
                    if d.reply_mode == ReplyMode::Ephemeral {
                        "customcommand.privately"
                    } else {
                        "customcommand.in-channel"
                    }
                ),
            )
            .inline(),
        )
        .field(
            EmbedFieldBuilder::new(
                t!(lang, "customcommand.cooldown"),
                if d.cooldown_secs > 0 {
                    format!("{} s", d.cooldown_secs)
                } else {
                    t!(lang, "customcommand.no-cooldown")
                },
            )
            .inline(),
        )
        .field(
            EmbedFieldBuilder::new(
                t!(lang, "customcommand.replies"),
                t!(lang, "customcommand.reply-count", count = d.responses.len()),
            )
            .inline(),
        )
        .field(
            EmbedFieldBuilder::new(
                t!(lang, "customcommand.extras"),
                t!(
                    lang,
                    "customcommand.extras-value",
                    kind = t!(
                        lang,
                        if d.embed.is_some() {
                            "customcommand.embed"
                        } else {
                            "customcommand.plain"
                        }
                    ),
                    actions = t!(lang, "customcommand.action-count", count = d.actions.len())
                ),
            )
            .inline(),
        );
    if !d.description.is_empty() {
        embed = embed.description(&d.description);
    }
    let limits = [
        ("customcommand.limit-allow-roles", d.allow_roles.len()),
        ("customcommand.limit-deny-roles", d.deny_roles.len()),
        ("customcommand.limit-allow-channels", d.allow_channels.len()),
        ("customcommand.limit-deny-channels", d.deny_channels.len()),
    ];
    let limits: Vec<String> = limits
        .into_iter()
        .filter(|(_, n)| *n > 0)
        .map(|(key, n)| t!(lang, key, count = n))
        .collect();
    if !limits.is_empty() {
        embed = embed.field(EmbedFieldBuilder::new(
            t!(lang, "customcommand.restrictions"),
            limits.join("\n"),
        ));
    }
    cx.reply(ephemeral(embed.build(), Vec::new())).await
}

// /Audit

/// Writes a change to the audit log, as made from Discord. A failed write is logged, never shown: the
/// change itself is real.
async fn audit(cx: &Cx, guild: u64, action: &str, target: &str, before: Value, after: Value) {
    let Ok(actor) = cx.author() else { return };
    let (before, after) = (before.to_string(), after.to_string());
    let name = cx.author_display_name();
    let written = cx
        .app
        .db
        .audit(&NewAudit {
            guild,
            actor: actor.id.get(),
            actor_name: &name,
            source: AuditSource::Discord,
            action,
            target: Some(target),
            before: Some(&before),
            after: Some(&after),
        })
        .await;
    if let Err(error) = written {
        tracing::error!(error = ?error, guild, action, "a custom command change was made but not written to the audit log");
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    fn row(name: &str, uses: i64, enabled: bool) -> CommandRow {
        CommandRow {
            id: 1,
            guild_id: 1,
            def: CommandDef {
                name: name.into(),
                description: String::new(),
                responses: vec!["x".into()],
                embed: None,
                reply_mode: ReplyMode::Public,
                actions: Vec::new(),
                allow_roles: Vec::new(),
                deny_roles: Vec::new(),
                allow_channels: Vec::new(),
                deny_channels: Vec::new(),
                cooldown_secs: 0,
                enabled,
                allow_mentions: false,
            },
            uses,
            created_by: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn the_cooldown_box_takes_whole_seconds() {
        assert_eq!(cooldown_of(Lang::En, ""), Ok(0));
        assert_eq!(cooldown_of(Lang::En, " 30 "), Ok(30));
        assert_eq!(cooldown_of(Lang::En, "3600"), Ok(3_600));
        for bad in ["3601", "-1", "1.5", "soon", "1e3"] {
            assert!(cooldown_of(Lang::En, bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_list_pages_and_its_buttons_stop_at_the_ends() {
        let few = vec![row("a", 3, true), row("b", 0, false)];
        let (embed, components) = page_view(Lang::En, &few, 0);
        assert!(components.is_empty(), "one page needs no buttons");
        let text = embed.description.unwrap();
        assert!(
            text.contains("`/tag a` · used 3×") && text.contains("**off**"),
            "{text}"
        );
        let many: Vec<CommandRow> = (0..25).map(|n| row(&format!("c{n:02}"), 0, true)).collect();
        let (embed, components) = page_view(Lang::En, &many, 2);
        assert_eq!(embed.footer.unwrap().text, "Page 3 of 3");
        assert_eq!(embed.description.unwrap().lines().count(), 5);
        let Component::ActionRow(buttons) = &components[0] else {
            panic!("a row of buttons")
        };
        let ids: Vec<(String, bool)> = buttons
            .components
            .iter()
            .map(|c| match c {
                Component::Button(b) => (b.custom_id.clone().unwrap(), b.disabled),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(ids, [("tg:l:1".to_owned(), false), ("tg:l:3".to_owned(), true)]);
        let (embed, _) = page_view(Lang::En, &many, 99);
        assert_eq!(
            embed.footer.unwrap().text,
            "Page 3 of 3",
            "a page past the end is the last"
        );
        let (embed, _) = page_view(Lang::En, &[], 0);
        assert!(embed.description.unwrap().contains("No custom commands yet"));
    }

    #[test]
    fn the_bots_own_commands_are_reserved() {
        let names = reserved_names();
        for name in ["tag", "customcommand", "help", "config", "profile"] {
            assert!(names.iter().any(|n| n == name), "{name}");
        }
        assert!(check_name("help", names).is_err());
        assert!(check_name("rules", names).is_ok());
    }

    #[test]
    fn a_form_is_three_boxes_with_the_reply_first() {
        let boxes = form(Lang::En, Some("hi".into()), Some(String::new()), Some(0), 1);
        assert_eq!(boxes.len(), 3);
        let ids: Vec<String> = boxes
            .iter()
            .map(|c| match c {
                Component::Label(l) => match l.component.as_ref() {
                    Component::TextInput(t) => t.custom_id.clone(),
                    other => panic!("{other:?}"),
                },
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(ids, [RESPONSE_FIELD, DESCRIPTION_FIELD, COOLDOWN_FIELD]);
    }
}
