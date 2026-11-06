//! `/play derby`: six hermit crabs race, and the channel backs the one it likes.
//!
//! A lobby opens for a minute or so; everyone picks a crab (and, when the host asked for a stake, puts
//! the same shells on it). The race is a GIF, decided by a seed kept with the lobby, so every pod runs
//! the same race. Whoever backed the winner shares the pool; nobody backing it, or too few racers,
//! gives every stake back, so the house never takes a shell. A staked race is pure chance, so it is a
//! gambling-style game: off until a manager switches it on with `/config gambling`, and with a limit on
//! how much a player can be down on the day. Free races are open everywhere.

use std::{
    str::Split,
    sync::{Arc, OnceLock},
};

use image::RgbaImage;
use pinchora_db::{DerbyJoin, DerbyLeave, DerbyOutcome, DerbyResult, GameSession, NewGame, PENDING, PartyMember};
use pinchora_game::{
    party::derby::{self, CRABS, DAILY_LOSS_LIMIT, LOBBY_SECS, MAX_RACERS, MIN_STAKED_RACERS, ROSTER, Race},
    play::{Game, MAX_BET, MIN_BET},
};
use pinchora_render::{DERBY_LANES, DerbyCard, DerbyLane, DerbyScene, SpriteArt, Theme, derby_gif, tint_crab};
use twilight_model::{
    channel::message::{Component, component::ButtonStyle},
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFooterBuilder};

use super::{
    games::{channel_of, location, quiet},
    party::{card_embed, clean_name, count, rest, resting, reward_win, track_message, wait_text},
};
use crate::{
    App, Cx,
    game::scene::scaled_png,
    server_events::{button, row},
    util::{BRAND, SUCCESS, mention, relative, thousands},
};

const _: () = assert!(CRABS == DERBY_LANES);

/// The crab sprites: the hermit crab pet in six shell colours, facing right, and the same drawn twice
/// as big for the lobby card. Built once; the atlas never changes while the bot runs.
struct CrabArt {
    sprites: Vec<RgbaImage>,
    big: Vec<(Vec<u8>, u32, u32)>,
}

fn crab_art(app: &App) -> anyhow::Result<&'static CrabArt> {
    static ART: OnceLock<CrabArt> = OnceLock::new();
    if let Some(art) = ART.get() {
        return Ok(art);
    }
    let base = app.sprites.pet("pebblepouch_hermit");
    let sprites: Vec<RgbaImage> = ROSTER.iter().map(|crab| tint_crab(&base, crab.shell)).collect();
    let big = sprites
        .iter()
        .map(|sprite| scaled_png(sprite, 2))
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(ART.get_or_init(|| CrabArt { sprites, big }))
}

/// The race a lobby will run: its seed is the lobby's state.
fn race_of(session: &GameSession) -> Race {
    derby::run(session.state.parse().unwrap_or(session.id as u64))
}

/// `/play derby`: opens a lobby in this channel.
pub(crate) async fn start(cx: &Cx, bet: Option<i64>) -> anyhow::Result<()> {
    let me = cx.author()?.id.get();
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral("Races run inside a server.").await;
    };
    let bet = bet.unwrap_or(0);
    if bet != 0 && !cx.app.db.gambling_enabled(guild).await? {
        return cx
            .reply_ephemeral(
                "🎲 Races with shells on them are a gambling-style game, and those are **off** on this server. \
                 A manager can switch them on with `/config gambling`. A race for fun needs no stake: \
                 just leave `bet` empty.",
            )
            .await;
    }
    if bet != 0 && !(MIN_BET..=MAX_BET).contains(&bet) {
        return cx
            .reply_ephemeral(format!("A stake is {MIN_BET} to {MAX_BET} shells."))
            .await;
    }
    if let Some(left) = resting(&cx.app, "derbystart", me).await? {
        return cx
            .reply_ephemeral(format!("Easy there! You can open another race in {}.", wait_text(left)))
            .await;
    }
    let seed: u64 = rand::random();
    let Some(session) = cx
        .app
        .db
        .open_party_game(NewGame {
            kind: "derby",
            guild_id: guild,
            channel_id: channel_of(cx),
            host: me,
            guest: None,
            bet,
            status: PENDING,
            state: &seed.to_string(),
            ttl_secs: LOBBY_SECS,
        })
        .await?
    else {
        return cx
            .reply_ephemeral("A derby is already open in this channel. Join it, or wait for the next one!")
            .await;
    };
    rest(&cx.app, "derbystart", me, 20).await;
    let data = lobby_message(&cx.app, &session, &[]).await?;
    cx.reply(data).await?;
    track_message(cx, session.id);
    Ok(())
}

/// The lobby: the card with every lane and who backs it, and the buttons to pick a crab.
async fn lobby_message(
    app: &App,
    session: &GameSession,
    racers: &[PartyMember],
) -> anyhow::Result<InteractionResponseData> {
    let art = crab_art(app)?;
    let backers: Vec<Vec<String>> = (0..CRABS)
        .map(|crab| {
            racers
                .iter()
                .filter(|r| r.choice as usize == crab)
                .map(|r| r.name.clone())
                .collect()
        })
        .collect();
    let (stake, racer_count) = (session.bet, racers.len());
    let png = app
        .render(move |r| {
            let lanes: Vec<DerbyLane<'_>> = (0..CRABS)
                .map(|n| DerbyLane {
                    name: ROSTER[n].name,
                    accent: ROSTER[n].accent,
                    art: SpriteArt {
                        png: &art.big[n].0,
                        w: art.big[n].1,
                        h: art.big[n].2,
                    },
                    backers: &backers[n],
                })
                .collect();
            r.derby_card(&DerbyCard {
                lanes: &lanes,
                stake,
                racers: racer_count,
                status: "Pick your crab below, you can switch until the start",
                theme: Theme::reef(),
            })
        })
        .await?;
    let host = mention(session.host_id);
    let stake_line = if session.bet > 0 {
        format!(
            "Everyone puts **{}** shells on their crab, and those who back the winner share the pool. \
             If nobody backs it, every stake comes back.",
            thousands(session.bet)
        )
    } else {
        "No stakes, just bragging rights (and a few shells if you back the winner).".to_owned()
    };
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title("Hermit Crab Derby")
        .footer(EmbedFooterBuilder::new("Pick a crab, then watch them go."));
    let mut data = Cx::card(embed, "derby.png", png, lobby_buttons(session.id), false);
    data.content = Some(format!(
        "🦀 {host} opened a **Hermit Crab Derby**! {stake_line}\nThe race starts {}.",
        relative(session.expires_at)
    ));
    data.allowed_mentions = quiet();
    Ok(data)
}

/// Six crab buttons in two rows, then the host's Start and everyone's Leave.
fn lobby_buttons(id: i64) -> Vec<Component> {
    let crabs: Vec<Component> = ROSTER
        .iter()
        .enumerate()
        .map(|(n, crab)| {
            button(
                format!("dy:c:{id}:{n}"),
                crab.name,
                Some(crab.emoji),
                ButtonStyle::Secondary,
                false,
            )
        })
        .collect();
    let mut rows: Vec<Component> = crabs.chunks(3).map(|c| row(c.to_vec())).collect();
    rows.push(row(vec![
        button(
            format!("dy:s:{id}"),
            "Start the race",
            None,
            ButtonStyle::Success,
            false,
        ),
        button(format!("dy:l:{id}"), "Leave", None, ButtonStyle::Secondary, false),
    ]));
    rows
}

/// `dy:c:<id>:<crab>` picks a crab, `dy:s:<id>` starts the race (the host), `dy:l:<id>` leaves.
pub(crate) async fn on_button(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let action = args.next();
    let Some(id) = args.next().and_then(|v| v.parse::<i64>().ok()) else {
        return cx.reply_ephemeral("This race has expired.").await;
    };
    match action {
        Some("c") => match args.next().and_then(|v| v.parse::<usize>().ok()).filter(|c| *c < CRABS) {
            Some(crab) => on_pick(cx, id, crab).await,
            None => cx.reply_ephemeral("This button has expired.").await,
        },
        Some("s") => on_start(cx, id).await,
        Some("l") => on_leave(cx, id).await,
        _ => cx.reply_ephemeral("This button has expired.").await,
    }
}

/// The lobby `id`, if it is a derby that is still open for picks.
async fn open_lobby(cx: &Cx, id: i64) -> anyhow::Result<Option<GameSession>> {
    match cx.app.db.game(id).await?.filter(|s| s.kind == "derby") {
        Some(session) if session.status == PENDING && !session.expired() => Ok(Some(session)),
        Some(session) if session.status == PENDING => {
            cx.reply_ephemeral("The race is about to start, the gates are closed!")
                .await?;
            Ok(None)
        }
        _ => {
            cx.reply_ephemeral("This race is over.").await?;
            Ok(None)
        }
    }
}

async fn on_pick(cx: &Cx, id: i64, crab: usize) -> anyhow::Result<()> {
    let Some(session) = open_lobby(cx, id).await? else {
        return Ok(());
    };
    let me = cx.author()?.id.get();
    let joined = cx
        .app
        .db
        .derby_join(
            id,
            me,
            &clean_name(&cx.author_display_name()),
            crab as i16,
            MAX_RACERS,
            DAILY_LOSS_LIMIT,
        )
        .await?;
    match joined {
        DerbyJoin::Joined | DerbyJoin::Moved => {}
        DerbyJoin::Closed => {
            return cx
                .reply_ephemeral("The gates just closed, this race is starting!")
                .await;
        }
        DerbyJoin::Full => {
            return cx
                .reply_ephemeral(format!("This race is full: {MAX_RACERS} racers is the most."))
                .await;
        }
        DerbyJoin::Broke { balance } => {
            return cx
                .reply_ephemeral(format!(
                    "You only have **{}** shells, but the stake is **{}**. Try `/daily`, or join a race for fun!",
                    thousands(balance),
                    thousands(session.bet)
                ))
                .await;
        }
        DerbyJoin::Down { net } => {
            return cx
                .reply_ephemeral(format!(
                    "That stake would take you past today's limit of **{}** shells down on staked races \
                     (you're at {}). Join a race for fun, or come back tomorrow.",
                    thousands(DAILY_LOSS_LIMIT),
                    thousands(net)
                ))
                .await;
        }
    }
    let racers = cx.app.db.party_members(id).await?;
    cx.update(lobby_message(&cx.app, &session, &racers).await?).await
}

async fn on_leave(cx: &Cx, id: i64) -> anyhow::Result<()> {
    let Some(session) = open_lobby(cx, id).await? else {
        return Ok(());
    };
    match cx.app.db.derby_leave(id, cx.author()?.id.get()).await? {
        DerbyLeave::Left { .. } => {
            let racers = cx.app.db.party_members(id).await?;
            cx.update(lobby_message(&cx.app, &session, &racers).await?).await
        }
        DerbyLeave::NotIn => cx.reply_ephemeral("You haven't picked a crab in this race.").await,
        DerbyLeave::Closed => {
            cx.reply_ephemeral("The gates just closed, this race is starting!")
                .await
        }
    }
}

async fn on_start(cx: &Cx, id: i64) -> anyhow::Result<()> {
    let Some(session) = open_lobby(cx, id).await? else {
        return Ok(());
    };
    let me = cx.author()?.id.get();
    if session.host_id as u64 != me {
        return cx
            .reply_ephemeral(format!(
                "Only {} can start the race early. It starts on its own {}.",
                mention(session.host_id),
                relative(session.expires_at)
            ))
            .await;
    }
    let racers = cx.app.db.party_members(id).await?.len();
    let needed = if session.bet > 0 { MIN_STAKED_RACERS } else { 1 };
    if racers < needed {
        return cx
            .reply_ephemeral(format!(
                "Pick a crab first: a race needs at least {needed} racer{} to start, and yours has {racers}. \
                 It starts on its own {} either way.",
                if needed == 1 { "" } else { "s" },
                relative(session.expires_at)
            ))
            .await;
    }
    match finish(&cx.app, id, Some(me)).await? {
        Some(done) => cx.update(done.response()).await,
        None => cx.reply_ephemeral("This race is already running.").await,
    }
}

/// Ends a lobby whose clock ran out: runs the race and edits the lobby's message. False when somebody
/// else got there first.
pub(crate) async fn time_out(app: &Arc<App>, session: &GameSession) -> anyhow::Result<bool> {
    let Some(done) = finish(app, session.id, None).await? else {
        return Ok(false);
    };
    if let Some(at) = location(session) {
        done.publish(app, at).await;
    }
    Ok(true)
}

/// How a race ended, as the message that replaces the lobby.
struct Finished {
    content: String,
    /// The race as a GIF; `None` when the race was called off and the lobby only gets its text.
    gif: Option<Vec<u8>>,
}

fn embed() -> EmbedBuilder {
    EmbedBuilder::new()
        .color(SUCCESS)
        .title("Hermit Crab Derby")
        .footer(EmbedFooterBuilder::new("Click the spoiler once the crabs are done."))
}

impl Finished {
    fn response(self) -> InteractionResponseData {
        match self.gif {
            Some(gif) => {
                let mut data = Cx::card(embed(), "derby.gif", gif, Vec::new(), false);
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

    /// Puts the result on the lobby's message with the bot's token.
    async fn publish(self, app: &App, at: (u64, u64)) {
        match self.gif {
            Some(gif) => {
                super::party::edit_card(
                    app,
                    at,
                    Some(&self.content),
                    card_embed(embed(), "derby.gif"),
                    ("derby.gif", gif),
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

/// Runs the race of lobby `id`, pays out and says how it went: the host starting it early (`forced`)
/// or the clock running out. `None` when the lobby was already ended by someone else.
async fn finish(app: &Arc<App>, id: i64, forced: Option<u64>) -> anyhow::Result<Option<Finished>> {
    let Some(session) = app.db.game(id).await?.filter(|s| s.kind == "derby") else {
        return Ok(None);
    };
    let race = race_of(&session);
    let winner = race.winner() as i16;
    let Some(result) = app.db.derby_settle(id, forced, MIN_STAKED_RACERS, winner).await? else {
        return Ok(None);
    };
    let host = mention(result.session.host_id);
    if result.outcome == DerbyOutcome::Cancelled {
        let why = if result.racers.is_empty() {
            "nobody picked a crab".to_owned()
        } else {
            format!("a race with shells on it needs at least {MIN_STAKED_RACERS} racers")
        };
        let back = if result.session.bet > 0 && !result.racers.is_empty() {
            " The stake went back."
        } else {
            ""
        };
        return Ok(Some(Finished {
            content: format!("🦀 {host}'s derby was called off: {why}.{back}"),
            gif: None,
        }));
    }

    // Stats for everyone who raced; a friendly race pays the winners a small reward, once two or more
    // players took part so that nobody can farm a race alone.
    let winners: Vec<u64> = result
        .racers
        .iter()
        .filter(|r| r.choice == result.winner)
        .map(|r| r.user_id as u64)
        .collect();
    let played: Vec<(u64, bool)> = result
        .racers
        .iter()
        .map(|r| (r.user_id as u64, r.choice == result.winner))
        .collect();
    count(app, Game::Derby, &played).await;
    let mut rewards = Vec::new();
    if let DerbyOutcome::Friendly(winners) = &result.outcome
        && result.racers.len() >= 2
    {
        for user in winners {
            let paid = reward_win(app, *user, Game::Derby, "derby_win").await;
            rewards.push((*user, paid));
        }
    }
    let content = results_text(&host, &race, &result, &rewards, &winners);

    // The race is over and paid whatever happens to the picture: without one the text still tells it.
    let art = crab_art(app)?;
    let gif = app
        .render(move |_| {
            derby_gif(&DerbyScene {
                crabs: &art.sprites,
                accents: std::array::from_fn(|n| ROSTER[n].accent),
                names: std::array::from_fn(|n| ROSTER[n].name),
                frames: &race.frames,
                order: race.order,
            })
        })
        .await;
    if let Err(error) = &gif {
        tracing::warn!(%error, session = id, "derby gif not drawn");
    }
    Ok(Some(Finished { content, gif: gif.ok() }))
}

/// The line under the GIF: placings and who got what, hidden in spoilers so the crabs finish first.
fn results_text(host: &str, race: &Race, result: &DerbyResult, rewards: &[(u64, i64)], winners: &[u64]) -> String {
    let medals = ["🥇", "🥈", "🥉"];
    let placings: Vec<String> = race
        .order
        .iter()
        .enumerate()
        .map(|(place, &crab)| {
            let c = &ROSTER[crab];
            match medals.get(place) {
                Some(medal) if place == 0 => format!("{medal} {} **{}**", c.emoji, c.name),
                Some(medal) => format!("{medal} {} {}", c.emoji, c.name),
                None => format!("{} {}", c.emoji, c.name),
            }
        })
        .collect();
    let winner = &ROSTER[result.winner as usize];
    let list = |users: Vec<String>| {
        let shown = users.iter().take(12).cloned().collect::<Vec<_>>().join(" · ");
        match users.len().saturating_sub(12) {
            0 => shown,
            more => format!("{shown} and {more} more"),
        }
    };
    let backers = match &result.outcome {
        DerbyOutcome::Paid(paid) => {
            let pool: i64 = result.racers.iter().map(|r| r.amount).sum();
            format!(
                "💰 The backers of **{}** share the pool of **{}** shells: {}",
                winner.name,
                thousands(pool),
                list(
                    paid.iter()
                        .map(|(u, s)| format!("{} +{}", mention(u), thousands(*s)))
                        .collect()
                )
            )
        }
        DerbyOutcome::Refunded => format!("Nobody backed **{}**, so every stake went back.", winner.name),
        DerbyOutcome::Friendly(_) if winners.is_empty() => format!("Nobody backed **{}**.", winner.name),
        DerbyOutcome::Friendly(_) => {
            let paid = |user: &u64| rewards.iter().find(|(u, _)| u == user).map_or(0, |(_, s)| *s);
            format!(
                "🎉 Backed **{}**: {}",
                winner.name,
                list(
                    winners
                        .iter()
                        .map(|u| match paid(u) {
                            0 => mention(u),
                            shells => format!("{} +{shells}", mention(u)),
                        })
                        .collect()
                )
            )
        }
        DerbyOutcome::Cancelled => String::new(),
    };
    format!(
        "🏁 {host}'s **Hermit Crab Derby**, and they're off!\n||{}\n{backers}||",
        placings.join(" · ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(state: &str) -> GameSession {
        GameSession {
            id: 5,
            kind: "derby".into(),
            guild_id: 1,
            channel_id: 2,
            message_id: None,
            host_id: 3,
            guest_id: None,
            bet: 0,
            status: PENDING.into(),
            state: state.into(),
            expires_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn a_lobby_runs_the_race_its_seed_says_and_a_broken_state_still_runs_one() {
        let (a, b) = (race_of(&session("12345")), race_of(&session("12345")));
        assert_eq!((a.order, a.frames.len()), (b.order, b.frames.len()));
        assert_eq!(race_of(&session("junk")).order, race_of(&session("junk")).order);
    }

    #[test]
    fn the_buttons_fit_discord_and_name_every_crab() {
        let rows = lobby_buttons(77);
        assert_eq!(rows.len(), 3, "two rows of crabs and the controls");
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
        assert_eq!(ids.len(), 8);
        assert!(ids.iter().all(|id| id.len() <= 100 && id.starts_with("dy:")));
        assert!(ids.contains(&"dy:c:77:5".to_owned()) && ids.contains(&"dy:s:77".to_owned()));
    }

    #[test]
    fn the_result_hides_the_placings_in_a_spoiler_and_names_the_payouts() {
        let race = derby::run(9);
        let racers = vec![
            PartyMember {
                user_id: 10,
                choice: race.winner() as i16,
                amount: 40,
                name: "A".into(),
            },
            PartyMember {
                user_id: 11,
                choice: ((race.winner() + 1) % CRABS) as i16,
                amount: 40,
                name: "B".into(),
            },
        ];
        let result = DerbyResult {
            session: session("9"),
            racers,
            winner: race.winner() as i16,
            outcome: DerbyOutcome::Paid(vec![(10, 80)]),
        };
        let text = results_text("<@3>", &race, &result, &[], &[10]);
        let winner = ROSTER[race.winner()].name;
        assert!(text.contains(&format!("**{winner}**")) && text.contains("||") && text.contains("<@10> +80"));
        assert!(text.contains("pool of **80** shells"));
        assert_eq!(text.matches("||").count(), 2, "one spoiler, opened and closed");
    }
}
