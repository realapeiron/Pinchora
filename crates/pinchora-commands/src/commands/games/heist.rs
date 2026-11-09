//! `/play heist`: a crew of crabs against a sunken vault.
//!
//! The crew gathers in a lobby and everyone picks a role. Three stages (slip past the guards, crack the
//! lock, haul the loot out) each go better with a crew member whose role fits. Nobody ever loses shells:
//! the vault pays the crew from the sea's treasure, up to a daily limit, or pays nobody.

use std::{str::Split, sync::Arc};

use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use pinchora_db::{CrewJoin, GameSession, HeistSettled, NewGame, PENDING, PartyMember};
use pinchora_game::lang::lower;
use pinchora_game::party::heist;
use pinchora_game::party::heist::COOLDOWN_SECS;
use pinchora_game::party::heist::DAILY_SHELLS;
use pinchora_game::party::heist::Heist;
use pinchora_game::party::heist::LOBBY_SECS;
use pinchora_game::party::heist::MAX_CREW;
use pinchora_game::party::heist::MIN_CREW;
use pinchora_game::party::heist::Role;
use pinchora_game::party::heist::Tier;
use pinchora_game::party::heist::stage_title;
use pinchora_game::play::Game;
use pinchora_render::{CrewView, HeistCard, StageView, Theme};
use twilight_model::{
    channel::message::{Component, component::ButtonStyle},
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::EmbedBuilder;

use crate::App;
use crate::Cx;
use crate::commands::games::games::channel_of;
use crate::commands::games::games::location;
use crate::commands::games::games::quiet;
use crate::commands::games::games::session_lang;
use crate::commands::games::games::state_lang;
use crate::commands::games::games::state_with_lang;
use crate::commands::games::party::card_embed;
use crate::commands::games::party::clean_name;
use crate::commands::games::party::count;
use crate::commands::games::party::edit_card;
use crate::commands::games::party::rest;
use crate::commands::games::party::resting;
use crate::commands::games::party::track_message;
use crate::commands::games::party::wait_text_in;
use crate::community::server_events::button;
use crate::community::server_events::row;
use crate::infra::util::BRAND;
use crate::infra::util::DANGER;
use crate::infra::util::SUCCESS;
use crate::infra::util::WARN;
use crate::infra::util::mention;
use crate::infra::util::relative;
use crate::infra::util::thousands_in;

/// The roles of a crew, in the order they joined.
fn roles_of(crew: &[PartyMember]) -> Vec<Role> {
    crew.iter()
        .filter_map(|m| Role::from_index(i64::from(m.choice)))
        .collect()
}

/// The heist a lobby will play: its seed is the lobby's state.
fn heist_of(session: &GameSession, crew: &[PartyMember]) -> Heist {
    heist::run(
        state_lang(&session.state).0.parse().unwrap_or(session.id as u64),
        &roles_of(crew),
    )
}

/// `/play heist`: opens a lobby in this channel.
pub(crate) async fn start(cx: &Cx) -> anyhow::Result<()> {
    let me = cx.author()?.id.get();
    let lang = cx.lang();
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral(t!(lang, "heist.server-only")).await;
    };
    // The lobby is public and keeps speaking the server's language to its end; `lang` is for the opener's refusals.
    let board_lang = cx.app.guild_lang(guild).await;
    if let Some(left) = resting(&cx.app, "heist", me).await? {
        return cx
            .reply_ephemeral(t!(lang, "heist.laying-low", wait = wait_text_in(lang, left)))
            .await;
    }
    let seed: u64 = rand::random();
    let Some(session) = cx
        .app
        .db
        .open_party_game(NewGame {
            kind: "heist",
            guild_id: guild,
            channel_id: channel_of(cx),
            host: me,
            guest: None,
            bet: 0,
            status: PENDING,
            state: &state_with_lang(&seed.to_string(), board_lang),
            ttl_secs: LOBBY_SECS,
        })
        .await?
    else {
        return cx.reply_ephemeral(t!(lang, "heist.already-planned")).await;
    };
    let data = lobby_message(&cx.app, &session, &[], board_lang).await?;
    cx.reply(data).await?;
    track_message(cx, session.id);
    Ok(())
}

/// A stage as the card will show it, with its words already in the player's language.
struct StageText {
    title: String,
    chance: u8,
    passed: Option<bool>,
    line: String,
}

/// The cards's stages for a crew that has not set off: no results, only the chances it has now.
fn stages_for(roles: &[Role], heist: Option<&Heist>, lang: Lang) -> [StageText; 3] {
    std::array::from_fn(|n| match heist {
        Some(heist) => StageText {
            title: stage_title(lang, n),
            chance: heist.stages[n].chance,
            passed: Some(heist.stages[n].passed),
            line: heist.stages[n].line_in(lang),
        },
        None => StageText {
            title: stage_title(lang, n),
            chance: heist::chance(roles, n),
            passed: None,
            line: String::new(),
        },
    })
}

async fn card(
    app: &App,
    crew: &[PartyMember],
    heist: Option<&Heist>,
    headline: String,
    each: i64,
    lang: Lang,
) -> anyhow::Result<Vec<u8>> {
    let names: Vec<(String, usize)> = crew
        .iter()
        .map(|m| {
            (
                m.name.clone(),
                Role::from_index(i64::from(m.choice)).map_or(0, Role::index),
            )
        })
        .collect();
    let roles = roles_of(crew);
    let stages = stages_for(&roles, heist, lang);
    let headline = lang.upper(&headline);
    app.render(move |r| {
        let views: Vec<CrewView<'_>> = names
            .iter()
            .map(|(name, role)| CrewView { name, role: *role })
            .collect();
        let stages = stages.each_ref().map(|stage| StageView {
            title: &stage.title,
            chance: stage.chance,
            passed: stage.passed,
            line: &stage.line,
        });
        r.heist_card(&HeistCard {
            lang,
            crew: &views,
            stages,
            headline: &headline,
            each,
            theme: Theme::reef(),
        })
    })
    .await
}

/// The lobby: the card with the crew and the chances they have now, and the buttons to pick a role.
async fn lobby_message(
    app: &App,
    session: &GameSession,
    crew: &[PartyMember],
    lang: Lang,
) -> anyhow::Result<InteractionResponseData> {
    let headline = t!(lang, "heist.card.crew-count", n = crew.len(), max = MAX_CREW);
    let png = card(app, crew, None, headline, 0, lang).await?;
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(lang, "heist.title"))
        .description(
            Role::ALL
                .map(|r| {
                    t!(
                        lang,
                        "heist.role-line",
                        emoji = r.emoji(),
                        role = r.label_in(lang),
                        job = r.job_in(lang)
                    )
                })
                .join("\n"),
        );
    let mut data = Cx::card(embed, "heist.png", png, lobby_buttons(session.id, lang), false);
    data.content = Some(t!(
        lang,
        "heist.lobby",
        host = mention(session.host_id),
        min = MIN_CREW,
        when = relative(session.expires_at)
    ));
    data.allowed_mentions = quiet();
    Ok(data)
}

fn lobby_buttons(id: i64, lang: Lang) -> Vec<Component> {
    vec![
        row(Role::ALL
            .iter()
            .map(|role| {
                button(
                    format!("hs:j:{id}:{}", role.index()),
                    &role.label_in(lang),
                    Some(role.emoji()),
                    ButtonStyle::Secondary,
                    false,
                )
            })
            .collect()),
        row(vec![
            button(
                format!("hs:s:{id}"),
                &t!(lang, "heist.set-off"),
                None,
                ButtonStyle::Success,
                false,
            ),
            button(
                format!("hs:l:{id}"),
                &t!(lang, "heist.leave"),
                None,
                ButtonStyle::Secondary,
                false,
            ),
        ]),
    ]
}

/// `hs:j:<id>:<role>` joins or changes role, `hs:s:<id>` sets off (the host), `hs:l:<id>` leaves.
pub(crate) async fn on_button(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let action = args.next();
    let Some(id) = args.next().and_then(|v| v.parse::<i64>().ok()) else {
        return cx.reply_ephemeral(t!(cx.lang(), "heist.expired")).await;
    };
    match action {
        Some("j") => match args
            .next()
            .and_then(|v| v.parse::<i64>().ok())
            .and_then(Role::from_index)
        {
            Some(role) => on_join(cx, id, role).await,
            None => cx.reply_ephemeral(t!(cx.lang(), "arcade.button-expired")).await,
        },
        Some("s") => on_set_off(cx, id).await,
        Some("l") => on_leave(cx, id).await,
        _ => cx.reply_ephemeral(t!(cx.lang(), "arcade.button-expired")).await,
    }
}

/// The lobby `id`, if it is a heist still gathering its crew.
async fn open_lobby(cx: &Cx, id: i64) -> anyhow::Result<Option<GameSession>> {
    match cx.app.db.game(id).await?.filter(|s| s.kind == "heist") {
        Some(session) if session.status == PENDING && !session.expired() => Ok(Some(session)),
        Some(session) if session.status == PENDING => {
            cx.reply_ephemeral(t!(cx.lang(), "heist.setting-off")).await?;
            Ok(None)
        }
        _ => {
            cx.reply_ephemeral(t!(cx.lang(), "heist.over")).await?;
            Ok(None)
        }
    }
}

async fn on_join(cx: &Cx, id: i64, role: Role) -> anyhow::Result<()> {
    let Some(session) = open_lobby(cx, id).await? else {
        return Ok(());
    };
    let me = cx.author()?.id.get();
    let lang = cx.lang();
    if let Some(left) = resting(&cx.app, "heist", me).await? {
        return cx
            .reply_ephemeral(t!(lang, "heist.laying-low", wait = wait_text_in(lang, left)))
            .await;
    }
    let joined = cx
        .app
        .db
        .heist_join(
            id,
            me,
            &clean_name(&cx.author_display_name()),
            role.index() as i16,
            MAX_CREW,
        )
        .await?;
    match joined {
        CrewJoin::Joined | CrewJoin::Moved => {}
        CrewJoin::Closed => return cx.reply_ephemeral(t!(lang, "heist.closed")).await,
        CrewJoin::Full => {
            return cx.reply_ephemeral(t!(lang, "heist.full", max = MAX_CREW)).await;
        }
    }
    let crew = cx.app.db.party_members(id).await?;
    let table = session_lang(&cx.app, &session).await;
    cx.update(lobby_message(&cx.app, &session, &crew, table).await?).await
}

async fn on_leave(cx: &Cx, id: i64) -> anyhow::Result<()> {
    let Some(session) = open_lobby(cx, id).await? else {
        return Ok(());
    };
    if !cx.app.db.heist_leave(id, cx.author()?.id.get()).await? {
        return cx.reply_ephemeral(t!(cx.lang(), "heist.not-in")).await;
    }
    let crew = cx.app.db.party_members(id).await?;
    let table = session_lang(&cx.app, &session).await;
    cx.update(lobby_message(&cx.app, &session, &crew, table).await?).await
}

async fn on_set_off(cx: &Cx, id: i64) -> anyhow::Result<()> {
    let Some(session) = open_lobby(cx, id).await? else {
        return Ok(());
    };
    let me = cx.author()?.id.get();
    let lang = cx.lang();
    if session.host_id as u64 != me {
        return cx
            .reply_ephemeral(t!(
                lang,
                "heist.host-only",
                host = mention(session.host_id),
                when = relative(session.expires_at)
            ))
            .await;
    }
    let crew = cx.app.db.party_members(id).await?.len();
    if crew < MIN_CREW {
        return cx
            .reply_ephemeral(t!(
                lang,
                "heist.too-few",
                min = MIN_CREW,
                crew = crew,
                when = relative(session.expires_at)
            ))
            .await;
    }
    let table = session_lang(&cx.app, &session).await;
    match finish(&cx.app, id, Some(me), table).await? {
        Some(done) => cx.update(done.response(table)).await,
        None => cx.reply_ephemeral(t!(lang, "heist.under-way")).await,
    }
}

/// Ends a lobby whose clock ran out: plays the heist and edits the lobby's message. False when
/// somebody else got there first.
pub(crate) async fn time_out(app: &Arc<App>, session: &GameSession) -> anyhow::Result<bool> {
    let lang = session_lang(app, session).await;
    let Some(done) = finish(app, session.id, None, lang).await? else {
        return Ok(false);
    };
    if let Some(at) = location(session) {
        done.publish(app, at, lang).await;
    }
    Ok(true)
}

/// How a heist ended, as the message that replaces the lobby.
struct Finished {
    content: String,
    /// The card with the stages and the crew; `None` when the heist was called off.
    card: Option<(Vec<u8>, Tier)>,
}

fn embed(tier: Tier, lang: Lang) -> EmbedBuilder {
    EmbedBuilder::new()
        .color(match tier {
            Tier::Vault => SUCCESS,
            Tier::Haul | Tier::Scraps => WARN,
            Tier::Busted => DANGER,
        })
        .title(t!(lang, "heist.title"))
}

impl Finished {
    fn response(self, lang: Lang) -> InteractionResponseData {
        match self.card {
            Some((png, tier)) => {
                let mut data = Cx::card(embed(tier, lang), "heist.png", png, Vec::new(), false);
                data.content = Some(self.content);
                data.allowed_mentions = quiet();
                data
            }
            None => InteractionResponseData {
                content: Some(self.content),
                components: Some(Vec::new()),
                allowed_mentions: quiet(),
                ..Default::default()
            },
        }
    }

    async fn publish(self, app: &App, at: (u64, u64), lang: Lang) {
        match self.card {
            Some((png, tier)) => {
                edit_card(
                    app,
                    at,
                    Some(&self.content),
                    card_embed(embed(tier, lang), "heist.png"),
                    ("heist.png", png),
                    &[],
                )
                .await;
            }
            None => {
                crate::commands::games::games::edit_message(app, at.0, at.1, Some(&self.content), None, Some(&[]))
                    .await;
            }
        }
    }
}

/// Sets lobby `id` off: plays the heist, pays the crew and says how it went. `None` when the lobby
/// was already ended by someone else.
async fn finish(app: &Arc<App>, id: i64, forced: Option<u64>, lang: Lang) -> anyhow::Result<Option<Finished>> {
    let Some(session) = app.db.game(id).await?.filter(|s| s.kind == "heist") else {
        return Ok(None);
    };
    let Some(settled) = app
        .db
        .heist_settle(id, forced, MIN_CREW, DAILY_SHELLS, |session, crew| {
            heist_of(session, crew).tier.each()
        })
        .await?
    else {
        return Ok(None);
    };
    let host = mention(session.host_id);
    let (crew, each, paid) = match settled {
        HeistSettled::CalledOff { .. } => {
            return Ok(Some(Finished {
                content: t!(lang, "heist.called-off", host = host, min = MIN_CREW),
                card: None,
            }));
        }
        HeistSettled::Done { crew, each, paid, .. } => (crew, each, paid),
    };
    let heist = heist_of(&session, &crew);
    for member in &crew {
        rest(app, "heist", member.user_id as u64, COOLDOWN_SECS).await;
    }
    let busted = heist.tier == Tier::Busted;
    let played: Vec<(u64, bool)> = crew.iter().map(|m| (m.user_id as u64, !busted)).collect();
    count(app, Game::Heist, &played).await;
    let lines: Vec<String> = crew
        .iter()
        .map(|m| {
            let role = Role::from_index(i64::from(m.choice)).map_or("", Role::emoji);
            let got = paid.iter().find(|(u, _)| *u == m.user_id as u64).map_or(0, |(_, s)| *s);
            match got {
                0 if busted => format!("{role} {}", mention(m.user_id)),
                0 => format!("{role} {} {}", mention(m.user_id), t!(lang, "heist.limit-reached")),
                shells => format!("{role} {} **+{}**", mention(m.user_id), thousands_in(lang, shells)),
            }
        })
        .collect();
    let content = t!(
        lang,
        "heist.done",
        host = host,
        headline = lower(lang, &heist.tier.headline_in(lang)),
        lines = lines.join("\n")
    );
    // The heist is over and paid whatever happens to the picture: without one the text still tells it.
    let png = card(app, &crew, Some(&heist), heist.tier.headline_in(lang), each, lang).await;
    if let Err(error) = &png {
        tracing::warn!(%error, session = id, "heist card not drawn");
    }
    Ok(Some(Finished {
        content,
        card: png.ok().map(|png| (png, heist.tier)),
    }))
}

#[cfg(test)]
mod tests {
    use crate::commands::games::heist::*;

    fn member(user: i64, role: Role) -> PartyMember {
        PartyMember {
            user_id: user,
            choice: role.index() as i16,
            amount: 0,
            name: format!("P{user}"),
        }
    }

    #[test]
    fn the_buttons_fit_discord_and_carry_every_role() {
        let rows = lobby_buttons(9, Lang::En);
        assert_eq!(rows.len(), 2);
        let ids: Vec<String> = rows
            .iter()
            .flat_map(|r| match r {
                Component::ActionRow(r) => r.components.iter().collect::<Vec<_>>(),
                _ => Vec::new(),
            })
            .filter_map(|c| match c {
                Component::Button(b) => b.custom_id.clone(),
                _ => None,
            })
            .collect();
        assert_eq!(ids, ["hs:j:9:0", "hs:j:9:1", "hs:j:9:2", "hs:s:9", "hs:l:9"]);
    }

    #[test]
    fn a_lobby_plays_the_heist_its_seed_says_for_the_crew_it_has() {
        let crew = [
            member(1, Role::Lookout),
            member(2, Role::Locksmith),
            member(3, Role::Diver),
        ];
        let session = GameSession {
            id: 4,
            kind: "heist".into(),
            guild_id: 1,
            channel_id: 1,
            message_id: None,
            host_id: 1,
            guest_id: None,
            bet: 0,
            status: PENDING.into(),
            state: "77".into(),
            expires_at: chrono::Utc::now(),
        };
        let (a, b) = (heist_of(&session, &crew), heist_of(&session, &crew));
        assert_eq!(a.tier, b.tier);
        assert_eq!(a.stages[0].chance, 64);
        // The stages shown before the start carry the chances but no results.
        let roles = roles_of(&crew);
        let lobby = stages_for(&roles, None, Lang::En);
        assert!(
            lobby
                .iter()
                .all(|s| s.passed.is_none() && s.chance == 64 && s.line.is_empty())
        );
    }
}
