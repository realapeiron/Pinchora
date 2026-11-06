//! Partners: two players who decorate each other's house, with a card of what they share.

use std::str::Split;

use chrono::Utc;
use pinchora_db::PairOutcome;
use pinchora_render::{
    encode_png,
    social::{Interaction, Person, interaction_still},
};
use twilight_interactions::command::{CommandModel, CreateCommand, ResolvedUser};
use twilight_model::{
    channel::message::{MessageFlags, component::ButtonStyle},
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder, EmbedFooterBuilder};

use super::{
    house,
    social::{self, Request, button, character_of, row},
};
use crate::{
    App, Cx,
    util::{BRAND, DANGER, SUCCESS, mention, thousands},
};

/// How long a partner request can be answered, in seconds.
const REQUEST_SECS: i64 = 10 * 60;
/// The moment of the hug scene that shows the pair together, with hearts above them.
const TOGETHER: f32 = 0.6;

#[derive(CommandModel, CreateCommand)]
#[command(name = "partner", desc = "Pair up with someone to decorate each other's house")]
pub enum PartnerCommand {
    #[command(name = "propose")]
    Propose(PartnerPropose),
    #[command(name = "view")]
    View(PartnerView),
    #[command(name = "leave")]
    Leave(PartnerLeave),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "propose", desc = "Ask someone to be your partner")]
pub struct PartnerPropose {
    /// Who to ask
    user: ResolvedUser,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "view", desc = "See a pair: since when, days together, what they share")]
pub struct PartnerView {
    /// Whose partnership (default: yours)
    user: Option<ResolvedUser>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "leave", desc = "End your partnership")]
pub struct PartnerLeave;

/// The remembered name of the partner of `user`, for the profile card; `None` without a partner.
pub async fn partner_name(app: &App, user: u64) -> Option<String> {
    app.db
        .partner_name(user)
        .await
        .map_err(|error| tracing::debug!(%error, "partner name not read"))
        .ok()
        .flatten()
}

impl PartnerCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let me = cx.author()?.id.get();
        match self {
            Self::Propose(c) => {
                let other = &c.user.resolved;
                let to = other.id.get();
                if other.bot || to == me {
                    return cx
                        .reply_ephemeral("A partner is somebody else. Ask someone from the server!")
                        .await;
                }
                if let Some(p) = cx.app.db.partner(me).await? {
                    return cx
                        .reply_ephemeral(format!(
                            "You already have a partner: {}. End it with `/partner leave` first.",
                            mention(p.partner_id)
                        ))
                        .await;
                }
                if cx.app.db.partner(to).await?.is_some() {
                    return cx
                        .reply_ephemeral(format!("{} already has a partner.", mention(to)))
                        .await;
                }
                if social::asked_too_soon(cx, "partner", me, to).await? {
                    return Ok(());
                }
                social::remember(cx, other).await?;
                let request = Request {
                    from: me,
                    to,
                    until: cx.app.now().timestamp() + REQUEST_SECS,
                };
                let embed = EmbedBuilder::new()
                    .color(BRAND)
                    .title("💞 Partner request")
                    .description(format!(
                        "{} asks {} to be partners!\nPartners decorate each other's house (`/house edit owner:`), show on each other's profile and share a page of their own in `/partner view`.\nThis request expires <t:{}:R>.",
                        mention(me),
                        mention(to),
                        request.until
                    ))
                    .build();
                cx.reply(InteractionResponseData {
                    content: Some(mention(to)),
                    embeds: Some(vec![embed]),
                    components: Some(vec![request.buttons("pt", "Accept")]),
                    allowed_mentions: social::ping_only(to),
                    ..Default::default()
                })
                .await
            }
            Self::View(c) => {
                if c.user.as_ref().is_some_and(|u| u.resolved.bot) {
                    return cx.reply_ephemeral("Bots don't have partners.").await;
                }
                let owner = c.user.as_ref().map_or(me, |u| u.resolved.id.get());
                if super::privacy::private_profile(cx, owner).await? {
                    return Ok(());
                }
                match &c.user {
                    Some(other) => social::remember(cx, &other.resolved).await?,
                    None => cx.app.db.remember_name(me, &social::plain_name(cx.author()?)).await?,
                }
                let Some(pair) = cx.app.db.partner(owner).await? else {
                    let text = if owner == me {
                        "You don't have a partner yet. Ask someone with `/partner propose`.".to_owned()
                    } else {
                        format!("{} doesn't have a partner.", mention(owner))
                    };
                    return cx.reply_ephemeral(text).await;
                };
                let data = card(cx, owner, pair.partner_id, pair.since).await?;
                cx.reply(data).await
            }
            Self::Leave(_) => {
                let Some(pair) = cx.app.db.partner(me).await? else {
                    return cx.reply_ephemeral("You don't have a partner.").await;
                };
                let embed = EmbedBuilder::new()
                    .color(DANGER)
                    .title("Leave your partnership?")
                    .description(format!(
                        "You and {} will no longer decorate each other's houses. Everything in a house stays with the house's owner, whoever placed it.",
                        mention(pair.partner_id)
                    ))
                    .build();
                cx.reply(InteractionResponseData {
                    embeds: Some(vec![embed]),
                    components: Some(vec![row(vec![
                        button(
                            "Leave",
                            format!("pt:leave:{me}:{}", pair.partner_id),
                            ButtonStyle::Danger,
                        ),
                        button("Stay together", "pt:stay".to_owned(), ButtonStyle::Secondary),
                    ])]),
                    flags: Some(MessageFlags::EPHEMERAL),
                    ..Default::default()
                })
                .await
            }
        }
    }
}

/// The pair's page: both characters together, since when, and what they share.
async fn card(cx: &Cx, a: u64, b: u64, since: chrono::DateTime<Utc>) -> anyhow::Result<InteractionResponseData> {
    let db = &cx.app.db;
    let (name_a, name_b) = (
        db.remembered_name(a).await?.unwrap_or_else(|| "Partner".to_owned()),
        db.remembered_name(b).await?.unwrap_or_else(|| "Partner".to_owned()),
    );
    let days = (cx.app.now() - since).num_days().max(0) + 1;
    let sent = db.exchanged(a, b).await?;
    let count = |kind: Interaction| {
        sent.iter()
            .find(|(k, ..)| k == kind.key())
            .map_or(0, |(_, ab, ba)| ab + ba)
    };
    let (likes, pieces) = db.pair_house_totals(a, b).await?;
    let png = together(&cx.app, a, b).await?;
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(format!("💞 {name_a} & {name_b}"))
        .field(EmbedFieldBuilder::new("Together since", format!("<t:{}:D>", since.timestamp())).inline())
        .field(EmbedFieldBuilder::new("Days together", thousands(days)).inline())
        .field(EmbedFieldBuilder::new(
            "Sent to each other",
            Interaction::ALL
                .map(|k| {
                    let n = count(k);
                    format!(
                        "{} {} {}",
                        k.emoji(),
                        thousands(n),
                        if n == 1 { k.noun() } else { plural(k) }
                    )
                })
                .join(" · "),
        ))
        .field(EmbedFieldBuilder::new(
            "Their houses",
            format!("💖 {} likes · 🪑 {} pieces", thousands(likes), thousands(pieces)),
        ))
        .footer(EmbedFooterBuilder::new(
            "Decorate each other's house with /house edit and the owner option.",
        ));
    Ok(Cx::card(embed, "partners.png", png, Vec::new(), false))
}

fn plural(kind: Interaction) -> &'static str {
    match kind {
        Interaction::Hug => "hugs",
        Interaction::Pat => "pats",
        Interaction::Wave => "waves",
        Interaction::HighFive => "high fives",
    }
}

/// Both characters in each other's arms, as a picture.
async fn together(app: &App, a: u64, b: u64) -> anyhow::Result<Vec<u8>> {
    let ((ca, _), (cb, _)) = (character_of(app, a).await?, character_of(app, b).await?);
    app.render(move |_| {
        let (spec_a, spec_b) = (ca.spec(), cb.spec());
        let still = interaction_still(
            Interaction::Hug,
            Person { spec: &spec_a },
            Person { spec: &spec_b },
            TOGETHER,
        );
        encode_png(&still)
    })
    .await
}

/// `pt:yes|no:<from>:<to>:<until>` and `pt:leave:<user>:<partner>` / `pt:stay` buttons. Anything
/// else is a click in the editor of a partner's house.
pub(crate) async fn on_component(cx: &Cx, args: Split<'_, char>, value: Option<&str>) -> anyhow::Result<()> {
    let mut peek = args.clone();
    let mut args = args;
    match peek.next() {
        Some(answer @ ("yes" | "no")) => {
            args.next();
            match Request::parse(&mut args) {
                Some(request) => on_answer(cx, answer == "yes", request).await,
                None => closed(cx, "This partner request has expired.").await,
            }
        }
        Some("leave") => {
            args.next();
            on_leave(cx, &mut args).await
        }
        Some("stay") => closed(cx, "You're still partners. 💞").await,
        _ => house::on_edit(cx, "pt", args, value).await,
    }
}

/// Replaces the message with a plain line and no buttons.
async fn closed(cx: &Cx, text: impl Into<String>) -> anyhow::Result<()> {
    cx.update(InteractionResponseData {
        content: Some(text.into()),
        embeds: Some(Vec::new()),
        components: Some(Vec::new()),
        ..Default::default()
    })
    .await
}

async fn on_answer(cx: &Cx, accept: bool, request: Request) -> anyhow::Result<()> {
    let me = cx.author()?.id.get();
    if me != request.to && !(me == request.from && !accept) {
        return cx.reply_ephemeral("This request isn't for you.").await;
    }
    if request.expired(&cx.app) {
        return closed(cx, "This partner request has expired.").await;
    }
    if !accept {
        let text = if me == request.from {
            format!("{} took the partner request back.", mention(request.from))
        } else {
            format!("{} said no thanks to {}.", mention(request.to), mention(request.from))
        };
        return closed(cx, text).await;
    }
    match cx.app.db.pair_partners(request.from, request.to).await? {
        PairOutcome::Paired { .. } => {
            let user = cx.author()?;
            cx.app.db.remember_name(me, &social::plain_name(user)).await?;
            let png = together(&cx.app, request.from, request.to).await?;
            let embed = EmbedBuilder::new()
                .color(SUCCESS)
                .title("💞 New partners")
                .description(format!(
                    "{} and {} are partners now!\nDecorate each other's house with `/house edit` and the `owner` option.",
                    mention(request.from),
                    mention(request.to)
                ));
            let mut data = Cx::card(embed, "partners.png", png, Vec::new(), false);
            data.content = Some(String::new());
            cx.update(data).await
        }
        PairOutcome::Taken { user } => {
            // A second click on Accept finds them already together.
            let together = cx
                .app
                .db
                .partner(request.from)
                .await?
                .is_some_and(|p| p.partner_id == request.to);
            let text = if together {
                format!(
                    "{} and {} are partners already.",
                    mention(request.from),
                    mention(request.to)
                )
            } else {
                format!(
                    "{} already has a partner, so this request can't be accepted.",
                    mention(user)
                )
            };
            closed(cx, text).await
        }
    }
}

async fn on_leave(cx: &Cx, args: &mut Split<'_, char>) -> anyhow::Result<()> {
    let me = cx.author()?.id.get();
    let user: u64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let partner: u64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    if user != me {
        return cx.reply_ephemeral("This button isn't yours.").await;
    }
    match cx.app.db.partner(me).await? {
        Some(p) if p.partner_id == partner => {
            cx.app.db.leave_partner(me).await?;
            closed(cx, format!("💔 You and {} are not partners anymore.", mention(partner))).await
        }
        _ => closed(cx, "You are not partners with them anymore.").await,
    }
}
