//! Opt-in birthdays and a separate Manage Server setup command.

use chrono::Datelike;
use pinchora_core::{
    birthdays::{self, BirthdaySettings},
    settings::{self, Grantor, RoleFacts},
    t,
};
use twilight_interactions::command::{CommandModel, CreateCommand, ResolvedUser};
use twilight_model::{
    application::interaction::InteractionChannel,
    application::{
        command::{CommandOptionChoice, CommandOptionChoiceValue},
        interaction::application_command::{CommandData, CommandOptionValue},
    },
    channel::message::{
        Component, MessageFlags,
        component::{ActionRow, Button, ButtonStyle},
    },
    guild::{Permissions, Role},
    http::interaction::InteractionResponseData,
};

use crate::Cx;

fn manage_guild() -> Permissions {
    Permissions::MANAGE_GUILD
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "birthday",
    desc = "Your birthday and upcoming celebrations",
    contexts = "guild"
)]
#[allow(clippy::large_enum_variant)] // parsed once per interaction
pub enum BirthdayCommand {
    #[command(name = "set")]
    Set(BirthdaySet),
    #[command(name = "remove")]
    Remove(BirthdayRemove),
    #[command(name = "me")]
    Me(BirthdayMe),
    #[command(name = "view")]
    View(BirthdayView),
    #[command(name = "next")]
    Next(BirthdayNext),
    #[command(name = "list")]
    List(BirthdayList),
}
#[derive(CommandModel, CreateCommand)]
#[command(
    name = "set",
    desc = "Save your birthday privately; you choose whether others can see it"
)]
pub struct BirthdaySet {
    /// Day of the month
    #[command(min_value = 1, max_value = 31)]
    day: i64,
    /// Month of the year
    #[command(min_value = 1, max_value = 12)]
    month: i64,
    /// Optional birth year, kept private
    #[command(min_value = 1900, max_value = 9999)]
    year: Option<i64>,
    /// IANA time zone; defaults to this server's zone
    #[command(autocomplete = true, max_length = 64)]
    timezone: Option<String>,
    /// Let others see the month and day
    public: Option<bool>,
    /// Let servers display your age when they allow it
    show_age: Option<bool>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "remove", desc = "Remove your birthday and stop future celebrations")]
pub struct BirthdayRemove;
#[derive(CommandModel, CreateCommand)]
#[command(name = "me", desc = "See the birthday and privacy choices you saved")]
pub struct BirthdayMe;
#[derive(CommandModel, CreateCommand)]
#[command(name = "view", desc = "See a member's birthday if they share it")]
pub struct BirthdayView {
    /// Member to view
    user: ResolvedUser,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "next", desc = "See the next public birthday in this server")]
pub struct BirthdayNext;
#[derive(CommandModel, CreateCommand)]
#[command(name = "list", desc = "Public birthdays in this server")]
pub struct BirthdayList {
    /// Optional month (1 to 12)
    #[command(min_value = 1, max_value = 12)]
    month: Option<i64>,
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "birthdays",
    desc = "Set up birthday celebrations for this server",
    default_permissions = "manage_guild",
    contexts = "guild"
)]
#[allow(clippy::large_enum_variant)] // parsed once per interaction
pub enum BirthdaysAdmin {
    #[command(name = "settings")]
    Settings(BirthdaysSettings),
    #[command(name = "preview")]
    Preview(BirthdaysPreview),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "settings", desc = "Set the birthday channel, role, message and gift")]
pub struct BirthdaysSettings {
    /// Turn announcements on or off
    enabled: Option<bool>,
    /// Where birthday cards are posted
    #[command(channel_types = "guild_text guild_announcement")]
    channel: Option<InteractionChannel>,
    /// Stop posting cards
    clear_channel: Option<bool>,
    /// Role members get for 24 hours
    role: Option<Role>,
    /// Stop granting a birthday role
    clear_role: Option<bool>,
    /// Message with {user} and optional {age}
    #[command(max_length = 500)]
    message: Option<String>,
    /// Server's IANA time zone, used as the default for /birthday set
    #[command(autocomplete = true, max_length = 64)]
    timezone: Option<String>,
    /// System birthday gift, 0 to 500 shells once per year globally
    #[command(min_value = 0, max_value = 500)]
    shells_gift: Option<i64>,
    /// Allow ages in messages when members also opt in
    allow_age: Option<bool>,
    /// Show the Cake Day badge on birthday cards
    badge: Option<bool>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "preview", desc = "Preview the birthday card and message")]
pub struct BirthdaysPreview;

impl BirthdayCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "error.guild-only")).await;
        };
        match self {
            Self::Set(s) => s.run(cx, guild).await,
            Self::Remove(_) => {
                let removed = cx.app.db.remove_birthday(cx.author()?.id.get()).await?;
                cx.reply_ephemeral(t!(
                    cx.lang(),
                    if removed { "birthdays.removed" } else { "birthdays.none" }
                ))
                .await
            }
            Self::Me(_) => show_me(cx).await,
            Self::View(v) => view(cx, v.user.resolved.id.get()).await,
            Self::Next(_) => upcoming(cx, guild, None, true).await,
            Self::List(l) => upcoming(cx, guild, l.month.map(|m| m as u32), false).await,
        }
    }
}
impl BirthdaySet {
    async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        let lang = cx.lang();
        let year = self.year.map(|n| n as i32);
        if !(1..=12).contains(&self.month)
            || !(1..=31).contains(&self.day)
            || self.year.is_some_and(|value| !(1900..=9999).contains(&value))
            || !birthdays::valid_date(self.month as u32, self.day as u32, year)
        {
            return cx.reply_ephemeral(t!(lang, "birthdays.invalid-date")).await;
        }
        let settings = cx.app.db.birthday_settings(guild).await?;
        let tz = self.timezone.unwrap_or(settings.server_tz);
        if birthdays::timezone(&tz).is_none() {
            return cx.reply_ephemeral(t!(lang, "birthdays.invalid-zone")).await;
        }
        let id = format!(
            "bd:s:{}:{}:{}:{}:{}:{}:{}",
            self.month,
            self.day,
            year.unwrap_or(0),
            tz,
            u8::from(self.public.unwrap_or(false)),
            u8::from(self.show_age.unwrap_or(false)),
            cx.author()?.id.get()
        );
        if id.len() > 100 {
            return cx.reply_ephemeral(t!(lang, "birthdays.invalid-zone")).await;
        }
        let row = Component::ActionRow(ActionRow {
            id: None,
            components: vec![Component::Button(Button {
                custom_id: Some(id),
                disabled: false,
                emoji: None,
                id: None,
                label: Some(t!(lang, "birthdays.confirm")),
                sku_id: None,
                style: ButtonStyle::Primary,
                url: None,
            })],
        });
        cx.reply(InteractionResponseData {
            content: Some(t!(
                lang,
                "birthdays.confirm-text",
                date = format!("{:02}/{:02}", self.day, self.month),
                zone = tz
            )),
            components: Some(vec![row]),
            flags: Some(MessageFlags::EPHEMERAL),
            ..Default::default()
        })
        .await
    }
}
async fn show_me(cx: &Cx) -> anyhow::Result<()> {
    let Some(b) = cx.app.db.birthday(cx.author()?.id.get()).await? else {
        return cx.reply_ephemeral(t!(cx.lang(), "birthdays.none")).await;
    };
    cx.reply_ephemeral(t!(
        cx.lang(),
        "birthdays.mine",
        date = b.year.map_or_else(
            || format!("{:02}/{:02}", b.day, b.month),
            |year| format!("{:02}/{:02}/{year}", b.day, b.month)
        ),
        zone = b.tz,
        visibility = t!(
            cx.lang(),
            if b.public {
                "birthdays.public"
            } else {
                "birthdays.private"
            }
        )
    ))
    .await
}
async fn view(cx: &Cx, user: u64) -> anyhow::Result<()> {
    if user == cx.author()?.id.get() {
        return show_me(cx).await;
    }
    let Some(b) = cx.app.db.birthday(user).await?.filter(|b| b.public) else {
        return cx.reply_ephemeral(t!(cx.lang(), "birthdays.hidden")).await;
    };
    let settings = cx.app.db.birthday_settings(cx.guild_id().unwrap()).await?;
    let age = birthdays::visible_age(
        b.year.map(i32::from),
        b.show_age,
        settings.allow_age,
        cx.app.now().year(),
    );
    cx.reply_ephemeral(t!(
        cx.lang(),
        "birthdays.view",
        user = format!("<@{user}>"),
        date = format!("{:02}/{:02}", b.day, b.month),
        age = age.map_or_else(|| "".into(), |n| t!(cx.lang(), "birthdays.age", age = n))
    ))
    .await
}
async fn upcoming(cx: &Cx, guild: u64, month: Option<u32>, first: bool) -> anyhow::Result<()> {
    let now = cx.app.now();
    let mut dates = cx
        .app
        .db
        .public_birthdays(guild)
        .await?
        .into_iter()
        .filter_map(|b| {
            if month.is_some_and(|m| m != u32::from(b.month as u16)) {
                return None;
            }
            let tz = birthdays::timezone(&b.tz)?;
            let next = birthdays::next_date(b.month as u32, b.day as u32, now, tz)?;
            let days = (next - birthdays::local_date(now, tz)).num_days();
            (days <= 30 || month.is_some() || first).then_some((days, b, next))
        })
        .collect::<Vec<_>>();
    dates.sort_by_key(|(days, b, _)| (*days, b.user_id));
    if dates.is_empty() {
        return cx.reply_ephemeral(t!(cx.lang(), "birthdays.no-upcoming")).await;
    }
    let text = dates
        .into_iter()
        .take(if first { 1 } else { 15 })
        .map(|(_, b, date)| format!("<@{}> · {:02}/{:02}", b.user_id, date.day(), date.month()))
        .collect::<Vec<_>>()
        .join("\n");
    cx.reply_ephemeral(t!(cx.lang(), "birthdays.upcoming", list = text))
        .await
}
impl BirthdaysAdmin {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "error.guild-only")).await;
        };
        if !cx.can_manage_guild() {
            return cx.reply_ephemeral(t!(cx.lang(), "error.needs-manage-server")).await;
        }
        match self {
            Self::Settings(s) => s.run(cx, guild).await,
            Self::Preview(_) => crate::birthdays::preview(cx, guild).await,
        }
    }
}
impl BirthdaysSettings {
    async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        let mut s = cx.app.db.birthday_settings(guild).await?;
        let touched = self.enabled.is_some()
            || self.channel.is_some()
            || self.clear_channel == Some(true)
            || self.role.is_some()
            || self.clear_role == Some(true)
            || self.message.is_some()
            || self.timezone.is_some()
            || self.shells_gift.is_some()
            || self.allow_age.is_some()
            || self.badge.is_some();
        if let Some(v) = self.enabled {
            s.enabled = v;
        }
        if let Some(c) = self.channel {
            s.channel_id = Some(c.id.get());
        }
        if self.clear_channel == Some(true) {
            s.channel_id = None;
            s.enabled = false;
        }
        if let Some(role) = self.role {
            if let Some(why) = role_refusal(cx, guild, &role).await? {
                return cx.reply_ephemeral(why).await;
            }
            s.role_id = Some(role.id.get());
        }
        if self.clear_role == Some(true) {
            s.role_id = None;
        }
        if let Some(v) = self.message {
            s.message = v;
        }
        if let Some(v) = self.timezone {
            s.server_tz = v;
        }
        if let Some(v) = self.shells_gift {
            if !(0..=i64::from(birthdays::MAX_GIFT)).contains(&v) {
                return cx
                    .reply_ephemeral(t!(cx.lang(), "birthdays.invalid-settings", field = "gift"))
                    .await;
            }
            s.shells_gift = v as i16;
        }
        if let Some(v) = self.allow_age {
            s.allow_age = v;
        }
        if let Some(v) = self.badge {
            s.badge = v;
        }
        if let Err(field) = birthdays::check_settings(&s) {
            return cx
                .reply_ephemeral(t!(cx.lang(), "birthdays.invalid-settings", field = field))
                .await;
        }
        if touched {
            cx.app.db.save_birthday_settings(guild, &s).await?;
        }
        cx.reply_ephemeral(settings_summary(cx.lang(), &s)).await
    }
}
pub(crate) async fn role_refusal(cx: &Cx, guild: u64, role: &Role) -> anyhow::Result<Option<String>> {
    let table = cx.app.role_table(guild).await?;
    let mine = cx.app.my_roles(guild).await?;
    let bot_top = crate::commands::moderation::top(&table, mine.iter().copied());
    let caller_top = crate::commands::moderation::top(
        &table,
        cx.interaction
            .member
            .as_ref()
            .map(|m| m.roles.iter().map(|r| r.get()).collect::<Vec<_>>())
            .unwrap_or_default(),
    );
    let grantor = Grantor {
        owner: cx
            .app
            .guild_facts(guild)
            .await
            .is_some_and(|f| f.owner == cx.author().ok().map_or(0, |u| u.id.get())),
        administrator: cx
            .interaction
            .member
            .as_ref()
            .and_then(|m| m.permissions)
            .is_some_and(|p| p.contains(Permissions::ADMINISTRATOR)),
        top_role: Some(caller_top),
    };
    let facts = RoleFacts {
        everyone: role.id.get() == guild,
        managed: role.managed,
        position: role.position,
        permissions: role.permissions.bits(),
    };
    let label = format!("<@&{}>", role.id);
    Ok(settings::role_refusal_in(cx.lang(), facts, Some(bot_top), &label)
        .or_else(|| settings::grant_refusal_in(cx.lang(), facts, grantor, &label)))
}
fn settings_summary(lang: pinchora_core::i18n::Lang, s: &BirthdaySettings) -> String {
    t!(
        lang,
        "birthdays.settings",
        state = t!(lang, if s.enabled { "birthdays.on" } else { "birthdays.off" }),
        channel = s.channel_id.map_or_else(|| "—".into(), |n| format!("<#{n}>")),
        role = s.role_id.map_or_else(|| "—".into(), |n| format!("<@&{n}>")),
        gift = s.shells_gift,
        zone = s.server_tz
    )
}
pub(crate) async fn on_component(cx: &Cx, mut parts: std::str::Split<'_, char>) -> anyhow::Result<()> {
    if parts.next() != Some("s") {
        return cx.reply_ephemeral(t!(cx.lang(), "component.button-expired")).await;
    }
    let parse = |part: Option<&str>| part.and_then(|s| s.parse::<i64>().ok());
    let (Some(month), Some(day), Some(year), Some(tz), Some(public), Some(age), Some(user)) = (
        parse(parts.next()),
        parse(parts.next()),
        parse(parts.next()),
        parts.next(),
        parse(parts.next()),
        parse(parts.next()),
        parse(parts.next()),
    ) else {
        return cx.reply_ephemeral(t!(cx.lang(), "component.button-expired")).await;
    };
    if user < 0 || user as u64 != cx.author()?.id.get() {
        return cx.reply_ephemeral(t!(cx.lang(), "birthdays.not-yours")).await;
    }
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || (year != 0 && !(1900..=9999).contains(&year))
        || !matches!((public, age), (0 | 1, 0 | 1))
        || !birthdays::valid_date(month as u32, day as u32, (year > 0).then_some(year as i32))
        || birthdays::timezone(tz).is_none()
    {
        return cx.reply_ephemeral(t!(cx.lang(), "birthdays.invalid-date")).await;
    }
    cx.app
        .db
        .set_birthday(
            user as u64,
            &pinchora_db::BirthdayInput {
                month: month as i16,
                day: day as i16,
                year: (year > 0).then_some(year as i16),
                tz,
                public: public == 1,
                show_age: age == 1,
            },
        )
        .await?;
    cx.reply_ephemeral(t!(cx.lang(), "birthdays.saved")).await
}

pub(crate) fn autocomplete(data: &CommandData) -> Vec<CommandOptionChoice> {
    let typed = data
        .options
        .iter()
        .flat_map(|o| match &o.value {
            CommandOptionValue::SubCommand(inner) => inner.iter().collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .find_map(|o| match &o.value {
            CommandOptionValue::Focused(text, _) if o.name == "timezone" => Some(text.to_lowercase()),
            _ => None,
        })
        .unwrap_or_default();
    chrono_tz::TZ_VARIANTS
        .iter()
        .map(ToString::to_string)
        .filter(|zone| zone.to_lowercase().contains(&typed))
        .take(25)
        .map(|zone| CommandOptionChoice {
            name: zone.clone(),
            name_localizations: None,
            value: CommandOptionChoiceValue::String(zone),
        })
        .collect()
}
