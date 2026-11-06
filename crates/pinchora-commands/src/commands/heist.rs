//! `/play heist`: a crew of crabs against a sunken vault.
//!
//! The crew gathers in a lobby and everyone picks a role. Three stages (slip past the guards, crack the
//! lock, haul the loot out) each go better with a crew member whose role fits. Nobody ever loses shells:
//! the vault pays the crew from the sea's treasure, up to a daily limit, or pays nobody.

use std::{str::Split, sync::Arc};

use pinchora_db::{CrewJoin, GameSession, HeistSettled, NewGame, PENDING, PartyMember};
use pinchora_game::{
    party::heist::{self, COOLDOWN_SECS, DAILY_SHELLS, Heist, LOBBY_SECS, MAX_CREW, MIN_CREW, Role, STAGES, Tier},
    play::Game,
};
use pinchora_render::{CrewView, HeistCard, StageView, Theme};
use twilight_model::{
    channel::message::{Component, component::ButtonStyle},
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::EmbedBuilder;

use super::{
    games::{channel_of, location, quiet},
    party::{card_embed, clean_name, count, edit_card, rest, resting, track_message, wait_text},
};
use crate::{
    App, Cx,
    server_events::{button, row},
    util::{BRAND, DANGER, SUCCESS, WARN, mention, relative, thousands},
};

/// The roles of a crew, in the order they joined.
fn roles_of(crew: &[PartyMember]) -> Vec<Role> {
    crew.iter()
        .filter_map(|m| Role::from_index(i64::from(m.choice)))
        .collect()
}

/// The heist a lobby will play: its seed is the lobby's state.
fn heist_of(session: &GameSession, crew: &[PartyMember]) -> Heist {
    heist::run(session.state.parse().unwrap_or(session.id as u64), &roles_of(crew))
}

/// `/play heist`: opens a lobby in this channel.
pub(crate) async fn start(cx: &Cx) -> anyhow::Result<()> {
    let me = cx.author()?.id.get();
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral("Heists happen inside a server.").await;
    };
    if let Some(left) = resting(&cx.app, "heist", me).await? {
        return cx
            .reply_ephemeral(format!(
                "You're laying low after your last heist. The guard crabs forget you in {}.",
                wait_text(left)
            ))
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
            state: &seed.to_string(),
            ttl_secs: LOBBY_SECS,
        })
        .await?
    else {
        return cx
            .reply_ephemeral("A heist is already being planned in this channel. Join the crew!")
            .await;
    };
    let data = lobby_message(&cx.app, &session, &[]).await?;
    cx.reply(data).await?;
    track_message(cx, session.id);
    Ok(())
}

/// The cards's stages for a crew that has not set off: no results, only the chances it has now.
fn stages_for(roles: &[Role], heist: Option<&Heist>) -> [StageView<'static>; 3] {
    std::array::from_fn(|n| match heist {
        Some(heist) => StageView {
            title: STAGES[n],
            chance: heist.stages[n].chance,
            passed: Some(heist.stages[n].passed),
            line: heist.stages[n].line,
        },
        None => StageView {
            title: STAGES[n],
            chance: heist::chance(roles, n),
            passed: None,
            line: "",
        },
    })
}

async fn card(
    app: &App,
    crew: &[PartyMember],
    heist: Option<&Heist>,
    headline: String,
    each: i64,
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
    let stages = stages_for(&roles, heist);
    app.render(move |r| {
        let views: Vec<CrewView<'_>> = names
            .iter()
            .map(|(name, role)| CrewView { name, role: *role })
            .collect();
        r.heist_card(&HeistCard {
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
) -> anyhow::Result<InteractionResponseData> {
    let png = card(app, crew, None, format!("CREW {}/{MAX_CREW}", crew.len()), 0).await?;
    let embed = EmbedBuilder::new().color(BRAND).title("Treasure Heist").description(
        Role::ALL
            .map(|r| format!("{} **{}** {}", r.emoji(), r.label(), r.job()))
            .join("\n"),
    );
    let mut data = Cx::card(embed, "heist.png", png, lobby_buttons(session.id), false);
    data.content = Some(format!(
        "💰 {} is planning a **Treasure Heist**! Pick a role with the buttons: a crew that covers all three has the \
         best chances. Nobody loses shells, the vault pays the crew or nobody. It needs at least {MIN_CREW} crew and \
         sets off {}.",
        mention(session.host_id),
        relative(session.expires_at)
    ));
    data.allowed_mentions = quiet();
    Ok(data)
}

fn lobby_buttons(id: i64) -> Vec<Component> {
    vec![
        row(Role::ALL
            .iter()
            .map(|role| {
                button(
                    format!("hs:j:{id}:{}", role.index()),
                    role.label(),
                    Some(role.emoji()),
                    ButtonStyle::Secondary,
                    false,
                )
            })
            .collect()),
        row(vec![
            button(format!("hs:s:{id}"), "Set off now", None, ButtonStyle::Success, false),
            button(format!("hs:l:{id}"), "Leave", None, ButtonStyle::Secondary, false),
        ]),
    ]
}

/// `hs:j:<id>:<role>` joins or changes role, `hs:s:<id>` sets off (the host), `hs:l:<id>` leaves.
pub(crate) async fn on_button(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let action = args.next();
    let Some(id) = args.next().and_then(|v| v.parse::<i64>().ok()) else {
        return cx.reply_ephemeral("This heist has expired.").await;
    };
    match action {
        Some("j") => match args
            .next()
            .and_then(|v| v.parse::<i64>().ok())
            .and_then(Role::from_index)
        {
            Some(role) => on_join(cx, id, role).await,
            None => cx.reply_ephemeral("This button has expired.").await,
        },
        Some("s") => on_set_off(cx, id).await,
        Some("l") => on_leave(cx, id).await,
        _ => cx.reply_ephemeral("This button has expired.").await,
    }
}

/// The lobby `id`, if it is a heist still gathering its crew.
async fn open_lobby(cx: &Cx, id: i64) -> anyhow::Result<Option<GameSession>> {
    match cx.app.db.game(id).await?.filter(|s| s.kind == "heist") {
        Some(session) if session.status == PENDING && !session.expired() => Ok(Some(session)),
        Some(session) if session.status == PENDING => {
            cx.reply_ephemeral("The crew is already setting off!").await?;
            Ok(None)
        }
        _ => {
            cx.reply_ephemeral("This heist is over.").await?;
            Ok(None)
        }
    }
}

async fn on_join(cx: &Cx, id: i64, role: Role) -> anyhow::Result<()> {
    let Some(session) = open_lobby(cx, id).await? else {
        return Ok(());
    };
    let me = cx.author()?.id.get();
    if let Some(left) = resting(&cx.app, "heist", me).await? {
        return cx
            .reply_ephemeral(format!(
                "You're laying low after your last heist. The guard crabs forget you in {}.",
                wait_text(left)
            ))
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
        CrewJoin::Closed => return cx.reply_ephemeral("The crew just set off without you!").await,
        CrewJoin::Full => {
            return cx
                .reply_ephemeral(format!("The crew is full: {MAX_CREW} is the most a heist holds."))
                .await;
        }
    }
    let crew = cx.app.db.party_members(id).await?;
    cx.update(lobby_message(&cx.app, &session, &crew).await?).await
}

async fn on_leave(cx: &Cx, id: i64) -> anyhow::Result<()> {
    let Some(session) = open_lobby(cx, id).await? else {
        return Ok(());
    };
    if !cx.app.db.heist_leave(id, cx.author()?.id.get()).await? {
        return cx.reply_ephemeral("You're not in this crew.").await;
    }
    let crew = cx.app.db.party_members(id).await?;
    cx.update(lobby_message(&cx.app, &session, &crew).await?).await
}

async fn on_set_off(cx: &Cx, id: i64) -> anyhow::Result<()> {
    let Some(session) = open_lobby(cx, id).await? else {
        return Ok(());
    };
    let me = cx.author()?.id.get();
    if session.host_id as u64 != me {
        return cx
            .reply_ephemeral(format!(
                "Only {} can set the crew off early. It sets off {}.",
                mention(session.host_id),
                relative(session.expires_at)
            ))
            .await;
    }
    let crew = cx.app.db.party_members(id).await?.len();
    if crew < MIN_CREW {
        return cx
            .reply_ephemeral(format!(
                "A heist needs at least {MIN_CREW} crew and yours has {crew}. Pick a role yourself, or wait for \
                 more to join; it sets off {} either way.",
                relative(session.expires_at)
            ))
            .await;
    }
    match finish(&cx.app, id, Some(me)).await? {
        Some(done) => cx.update(done.response()).await,
        None => cx.reply_ephemeral("This heist is already under way.").await,
    }
}

/// Ends a lobby whose clock ran out: plays the heist and edits the lobby's message. False when
/// somebody else got there first.
pub(crate) async fn time_out(app: &Arc<App>, session: &GameSession) -> anyhow::Result<bool> {
    let Some(done) = finish(app, session.id, None).await? else {
        return Ok(false);
    };
    if let Some(at) = location(session) {
        done.publish(app, at).await;
    }
    Ok(true)
}

/// How a heist ended, as the message that replaces the lobby.
struct Finished {
    content: String,
    /// The card with the stages and the crew; `None` when the heist was called off.
    card: Option<(Vec<u8>, Tier)>,
}

fn embed(tier: Tier) -> EmbedBuilder {
    EmbedBuilder::new()
        .color(match tier {
            Tier::Vault => SUCCESS,
            Tier::Haul | Tier::Scraps => WARN,
            Tier::Busted => DANGER,
        })
        .title("Treasure Heist")
}

impl Finished {
    fn response(self) -> InteractionResponseData {
        match self.card {
            Some((png, tier)) => {
                let mut data = Cx::card(embed(tier), "heist.png", png, Vec::new(), false);
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

    async fn publish(self, app: &App, at: (u64, u64)) {
        match self.card {
            Some((png, tier)) => {
                edit_card(
                    app,
                    at,
                    Some(&self.content),
                    card_embed(embed(tier), "heist.png"),
                    ("heist.png", png),
                    &[],
                )
                .await;
            }
            None => {
                super::games::edit_message(app, at.0, at.1, Some(&self.content), None, Some(&[])).await;
            }
        }
    }
}

/// Sets lobby `id` off: plays the heist, pays the crew and says how it went. `None` when the lobby
/// was already ended by someone else.
async fn finish(app: &Arc<App>, id: i64, forced: Option<u64>) -> anyhow::Result<Option<Finished>> {
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
                content: format!("🏴‍☠️ {host}'s heist was called off: a heist needs at least {MIN_CREW} crew."),
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
                0 => format!("{role} {} (today's limit is reached)", mention(m.user_id)),
                shells => format!("{role} {} **+{}**", mention(m.user_id), thousands(shells)),
            }
        })
        .collect();
    let content = format!(
        "💰 {host}'s heist is over: **{}**!\n{}",
        heist.tier.headline().to_lowercase(),
        lines.join("\n")
    );
    // The heist is over and paid whatever happens to the picture: without one the text still tells it.
    let png = card(app, &crew, Some(&heist), heist.tier.headline().to_owned(), each).await;
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
    use super::*;

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
        let rows = lobby_buttons(9);
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
        let lobby = stages_for(&roles, None);
        assert!(
            lobby
                .iter()
                .all(|s| s.passed.is_none() && s.chance == 64 && s.line.is_empty())
        );
    }
}
