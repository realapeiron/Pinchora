//! Partners: two players who decorate each other's house, with a card of what they share.

use std::str::Split;

use chrono::Utc;
use pinchora_core::t;
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
    util::{BRAND, DANGER, SUCCESS, mention, thousands_in},
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
    #[command(name = "streak")]
    PartnerStreak(super::reef_loop::PartnerStreak),
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
            Self::PartnerStreak(c) => c.run(cx).await,

            Self::Propose(c) => {
                let other = &c.user.resolved;
                let to = other.id.get();
                if other.bot || to == me {
                    return cx.reply_ephemeral(t!(cx.lang(), "partner.not-self")).await;
                }
                if let Some(p) = cx.app.db.partner(me).await? {
                    return cx
                        .reply_ephemeral(t!(cx.lang(), "partner.you-have", partner = mention(p.partner_id)))
                        .await;
                }
                if cx.app.db.partner(to).await?.is_some() {
                    return cx
                        .reply_ephemeral(t!(cx.lang(), "partner.they-have", user = mention(to)))
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
                let public_lang = match cx.guild_id() {
                    Some(guild) => cx.app.guild_lang(guild).await,
                    None => cx.lang(),
                };
                let embed = EmbedBuilder::new()
                    .color(BRAND)
                    .title(t!(public_lang, "partner.request-title"))
                    .description(t!(
                        public_lang,
                        "partner.request",
                        from = mention(me),
                        to = mention(to),
                        until = request.until
                    ))
                    .build();
                cx.reply(InteractionResponseData {
                    content: Some(mention(to)),
                    embeds: Some(vec![embed]),
                    components: Some(vec![request.buttons(
                        public_lang,
                        "pt",
                        &t!(public_lang, "partner.accept"),
                    )]),
                    allowed_mentions: social::ping_only(to),
                    ..Default::default()
                })
                .await
            }
            Self::View(c) => {
                if c.user.as_ref().is_some_and(|u| u.resolved.bot) {
                    return cx.reply_ephemeral(t!(cx.lang(), "partner.bot")).await;
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
                        t!(cx.lang(), "partner.none-self")
                    } else {
                        t!(cx.lang(), "partner.none-other", user = mention(owner))
                    };
                    return cx.reply_ephemeral(text).await;
                };
                let data = card(cx, owner, pair.partner_id, pair.since).await?;
                cx.reply(data).await
            }
            Self::Leave(_) => {
                let Some(pair) = cx.app.db.partner(me).await? else {
                    return cx.reply_ephemeral(t!(cx.lang(), "partner.none")).await;
                };
                let embed = EmbedBuilder::new()
                    .color(DANGER)
                    .title(t!(cx.lang(), "partner.leave-title"))
                    .description(t!(cx.lang(), "partner.leave-body", partner = mention(pair.partner_id)))
                    .build();
                cx.reply(InteractionResponseData {
                    embeds: Some(vec![embed]),
                    components: Some(vec![row(vec![
                        button(
                            &t!(cx.lang(), "partner.leave"),
                            format!("pt:leave:{me}:{}", pair.partner_id),
                            ButtonStyle::Danger,
                        ),
                        button(
                            &t!(cx.lang(), "partner.stay"),
                            "pt:stay".to_owned(),
                            ButtonStyle::Secondary,
                        ),
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
    let lang = cx.lang();
    let (name_a, name_b) = (
        db.remembered_name(a)
            .await?
            .unwrap_or_else(|| t!(lang, "partner.fallback-name")),
        db.remembered_name(b)
            .await?
            .unwrap_or_else(|| t!(lang, "partner.fallback-name")),
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
        .field(EmbedFieldBuilder::new(t!(lang, "partner.card-since"), format!("<t:{}:D>", since.timestamp())).inline())
        .field(EmbedFieldBuilder::new(t!(lang, "partner.card-days"), thousands_in(lang, days)).inline())
        .field(EmbedFieldBuilder::new(
            t!(lang, "partner.card-sent"),
            Interaction::ALL
                .map(|k| format!("{} {}", k.emoji(), super::interact::count_text(lang, k, count(k))))
                .join(" · "),
        ))
        .field(EmbedFieldBuilder::new(
            t!(lang, "partner.card-houses"),
            t!(
                lang,
                "partner.card-houses-value",
                likes = thousands_in(lang, likes),
                pieces = thousands_in(lang, pieces)
            ),
        ))
        .footer(EmbedFooterBuilder::new(t!(lang, "partner.card-footer")));
    Ok(Cx::card(embed, "partners.png", png, Vec::new(), false))
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
                None => closed(cx, t!(cx.lang(), "partner.expired")).await,
            }
        }
        Some("leave") => {
            args.next();
            on_leave(cx, &mut args).await
        }
        Some("stay") => closed(cx, t!(cx.lang(), "partner.stayed")).await,
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
    let public_lang = match cx.guild_id() {
        Some(guild) => cx.app.guild_lang(guild).await,
        None => cx.lang(),
    };
    if me != request.to && !(me == request.from && !accept) {
        return cx.reply_ephemeral(t!(cx.lang(), "social.not-for-you")).await;
    }
    if request.expired(&cx.app) {
        return closed(cx, t!(public_lang, "partner.expired")).await;
    }
    if !accept {
        let text = if me == request.from {
            t!(public_lang, "partner.withdrawn", from = mention(request.from))
        } else {
            t!(
                public_lang,
                "social.said-no",
                to = mention(request.to),
                from = mention(request.from)
            )
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
                .title(t!(public_lang, "partner.new-title"))
                .description(t!(
                    public_lang,
                    "partner.new",
                    from = mention(request.from),
                    to = mention(request.to)
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
                t!(
                    public_lang,
                    "partner.already-partners",
                    from = mention(request.from),
                    to = mention(request.to)
                )
            } else {
                t!(public_lang, "partner.taken", user = mention(user))
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
        return cx.reply_ephemeral(t!(cx.lang(), "partner.not-yours")).await;
    }
    match cx.app.db.partner(me).await? {
        Some(p) if p.partner_id == partner => {
            cx.app.db.leave_partner(me).await?;
            closed(cx, t!(cx.lang(), "partner.left", partner = mention(partner))).await
        }
        _ => closed(cx, t!(cx.lang(), "partner.not-partners")).await,
    }
}
