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
use pinchora_core::{i18n::Lang, t};
use pinchora_db::{DerbyJoin, DerbyLeave, DerbyOutcome, DerbyResult, GameSession, NewGame, PENDING, PartyMember};
use pinchora_game::{
    lang::upper,
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
    games::{channel_of, location, quiet, session_lang, state_lang, state_with_lang},
    party::{card_embed, clean_name, count, rest, resting, reward_win, track_message, wait_text_in},
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
    derby::run(state_lang(&session.state).0.parse().unwrap_or(session.id as u64))
}

/// `/play derby`: opens a lobby in this channel.
pub(crate) async fn start(cx: &Cx, bet: Option<i64>) -> anyhow::Result<()> {
    let me = cx.author()?.id.get();
    let lang = cx.lang();
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral(t!(lang, "derby.server-only")).await;
    };
    // The lobby is public and keeps speaking the server's language to its end; `lang` is for the opener's refusals.
    let board_lang = cx.app.guild_lang(guild).await;
    let bet = bet.unwrap_or(0);
    if bet != 0 && !cx.app.db.gambling_enabled(guild).await? {
        return cx.reply_ephemeral(t!(lang, "derby.gambling-off")).await;
    }
    if bet != 0 && !(MIN_BET..=MAX_BET).contains(&bet) {
        return cx
            .reply_ephemeral(t!(lang, "derby.stake-range", min = MIN_BET, max = MAX_BET))
            .await;
    }
    if let Some(left) = resting(&cx.app, "derbystart", me).await? {
        return cx
            .reply_ephemeral(t!(lang, "derby.easy", wait = wait_text_in(lang, left)))
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
            state: &state_with_lang(&seed.to_string(), board_lang),
            ttl_secs: LOBBY_SECS,
        })
        .await?
    else {
        return cx.reply_ephemeral(t!(lang, "derby.already-open")).await;
    };
    rest(&cx.app, "derbystart", me, 20).await;
    let data = lobby_message(&cx.app, &session, &[], board_lang).await?;
    cx.reply(data).await?;
    track_message(cx, session.id);
    Ok(())
}

/// The lobby: the card with every lane and who backs it, and the buttons to pick a crab.
async fn lobby_message(
    app: &App,
    session: &GameSession,
    racers: &[PartyMember],
    lang: Lang,
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
    let status = upper(lang, &t!(lang, "derby.lobby-status"));
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
                lang,
                lanes: &lanes,
                stake,
                racers: racer_count,
                status: &status,
                theme: Theme::reef(),
            })
        })
        .await?;
    let host = mention(session.host_id);
    let stake_line = if session.bet > 0 {
        t!(lang, "derby.lobby-stake", stake = thousands(session.bet))
    } else {
        t!(lang, "derby.lobby-free")
    };
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(lang, "derby.title"))
        .footer(EmbedFooterBuilder::new(t!(lang, "derby.lobby-footer")));
    let mut data = Cx::card(embed, "derby.png", png, lobby_buttons(session.id, lang), false);
    data.content = Some(t!(
        lang,
        "derby.lobby",
        host = host,
        stake_line = stake_line,
        when = relative(session.expires_at)
    ));
    data.allowed_mentions = quiet();
    Ok(data)
}

/// Six crab buttons in two rows, then the host's Start and everyone's Leave.
fn lobby_buttons(id: i64, lang: Lang) -> Vec<Component> {
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
            &t!(lang, "derby.start-button"),
            None,
            ButtonStyle::Success,
            false,
        ),
        button(
            format!("dy:l:{id}"),
            &t!(lang, "derby.leave-button"),
            None,
            ButtonStyle::Secondary,
            false,
        ),
    ]));
    rows
}

/// `dy:c:<id>:<crab>` picks a crab, `dy:s:<id>` starts the race (the host), `dy:l:<id>` leaves.
pub(crate) async fn on_button(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let action = args.next();
    let Some(id) = args.next().and_then(|v| v.parse::<i64>().ok()) else {
        return cx.reply_ephemeral(t!(cx.lang(), "derby.expired")).await;
    };
    match action {
        Some("c") => match args.next().and_then(|v| v.parse::<usize>().ok()).filter(|c| *c < CRABS) {
            Some(crab) => on_pick(cx, id, crab).await,
            None => cx.reply_ephemeral(t!(cx.lang(), "arcade.button-expired")).await,
        },
        Some("s") => on_start(cx, id).await,
        Some("l") => on_leave(cx, id).await,
        _ => cx.reply_ephemeral(t!(cx.lang(), "arcade.button-expired")).await,
    }
}

/// The lobby `id`, if it is a derby that is still open for picks.
async fn open_lobby(cx: &Cx, id: i64) -> anyhow::Result<Option<GameSession>> {
    match cx.app.db.game(id).await?.filter(|s| s.kind == "derby") {
        Some(session) if session.status == PENDING && !session.expired() => Ok(Some(session)),
        Some(session) if session.status == PENDING => {
            cx.reply_ephemeral(t!(cx.lang(), "derby.gates-closing")).await?;
            Ok(None)
        }
        _ => {
            cx.reply_ephemeral(t!(cx.lang(), "derby.over")).await?;
            Ok(None)
        }
    }
}

async fn on_pick(cx: &Cx, id: i64, crab: usize) -> anyhow::Result<()> {
    let Some(session) = open_lobby(cx, id).await? else {
        return Ok(());
    };
    let me = cx.author()?.id.get();
    let lang = cx.lang();
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
            return cx.reply_ephemeral(t!(lang, "derby.gates-closed")).await;
        }
        DerbyJoin::Full => {
            return cx.reply_ephemeral(t!(lang, "derby.full", max = MAX_RACERS)).await;
        }
        DerbyJoin::Broke { balance } => {
            return cx
                .reply_ephemeral(t!(
                    lang,
                    "derby.broke",
                    balance = thousands(balance),
                    stake = thousands(session.bet)
                ))
                .await;
        }
        DerbyJoin::Down { net } => {
            return cx
                .reply_ephemeral(t!(
                    lang,
                    "derby.down",
                    limit = thousands(DAILY_LOSS_LIMIT),
                    net = thousands(net)
                ))
                .await;
        }
    }
    let racers = cx.app.db.party_members(id).await?;
    let table = session_lang(&cx.app, &session).await;
    cx.update(lobby_message(&cx.app, &session, &racers, table).await?).await
}

async fn on_leave(cx: &Cx, id: i64) -> anyhow::Result<()> {
    let Some(session) = open_lobby(cx, id).await? else {
        return Ok(());
    };
    match cx.app.db.derby_leave(id, cx.author()?.id.get()).await? {
        DerbyLeave::Left { .. } => {
            let racers = cx.app.db.party_members(id).await?;
            let table = session_lang(&cx.app, &session).await;
            cx.update(lobby_message(&cx.app, &session, &racers, table).await?).await
        }
        DerbyLeave::NotIn => cx.reply_ephemeral(t!(cx.lang(), "derby.not-in")).await,
        DerbyLeave::Closed => cx.reply_ephemeral(t!(cx.lang(), "derby.gates-closed")).await,
    }
}

async fn on_start(cx: &Cx, id: i64) -> anyhow::Result<()> {
    let Some(session) = open_lobby(cx, id).await? else {
        return Ok(());
    };
    let me = cx.author()?.id.get();
    let lang = cx.lang();
    if session.host_id as u64 != me {
        return cx
            .reply_ephemeral(t!(
                lang,
                "derby.host-only",
                host = mention(session.host_id),
                when = relative(session.expires_at)
            ))
            .await;
    }
    let racers = cx.app.db.party_members(id).await?.len();
    let needed = if session.bet > 0 { MIN_STAKED_RACERS } else { 1 };
    if racers < needed {
        return cx
            .reply_ephemeral(t!(
                lang,
                "derby.need-racers",
                count = needed,
                racers = racers,
                when = relative(session.expires_at)
            ))
            .await;
    }
    let table = session_lang(&cx.app, &session).await;
    match finish(&cx.app, id, Some(me), table).await? {
        Some(done) => cx.update(done.response(table)).await,
        None => cx.reply_ephemeral(t!(lang, "derby.already-running")).await,
    }
}

/// Ends a lobby whose clock ran out: runs the race and edits the lobby's message. False when somebody
/// else got there first.
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

/// How a race ended, as the message that replaces the lobby.
struct Finished {
    content: String,
    /// The race as a GIF; `None` when the race was called off and the lobby only gets its text.
    gif: Option<Vec<u8>>,
}

fn embed(lang: Lang) -> EmbedBuilder {
    EmbedBuilder::new()
        .color(SUCCESS)
        .title(t!(lang, "derby.title"))
        .footer(EmbedFooterBuilder::new(t!(lang, "derby.result-footer")))
}

impl Finished {
    fn response(self, lang: Lang) -> InteractionResponseData {
        match self.gif {
            Some(gif) => {
                let mut data = Cx::card(embed(lang), "derby.gif", gif, Vec::new(), false);
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
    async fn publish(self, app: &App, at: (u64, u64), lang: Lang) {
        match self.gif {
            Some(gif) => {
                super::party::edit_card(
                    app,
                    at,
                    Some(&self.content),
                    card_embed(embed(lang), "derby.gif"),
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
async fn finish(app: &Arc<App>, id: i64, forced: Option<u64>, lang: Lang) -> anyhow::Result<Option<Finished>> {
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
            t!(lang, "derby.why-nobody")
        } else {
            t!(lang, "derby.why-few", min = MIN_STAKED_RACERS)
        };
        let back = if result.session.bet > 0 && !result.racers.is_empty() {
            t!(lang, "derby.stake-back")
        } else {
            String::new()
        };
        return Ok(Some(Finished {
            content: t!(lang, "derby.called-off", host = host, why = why, back = back),
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
    let content = results_text(&host, &race, &result, &rewards, &winners, lang);

    // The race is over and paid whatever happens to the picture: without one the text still tells it.
    let art = crab_art(app)?;
    let gif = app
        .render(move |_| {
            derby_gif(&DerbyScene {
                lang,
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
fn results_text(
    host: &str,
    race: &Race,
    result: &DerbyResult,
    rewards: &[(u64, i64)],
    winners: &[u64],
    lang: Lang,
) -> String {
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
            more => t!(lang, "derby.and-more", shown = shown, more = more),
        }
    };
    let backers = match &result.outcome {
        DerbyOutcome::Paid(paid) => {
            let pool: i64 = result.racers.iter().map(|r| r.amount).sum();
            t!(
                lang,
                "derby.paid",
                crab = winner.name,
                pool = thousands(pool),
                list = list(
                    paid.iter()
                        .map(|(u, s)| format!("{} +{}", mention(u), thousands(*s)))
                        .collect()
                )
            )
        }
        DerbyOutcome::Refunded => t!(lang, "derby.refunded", crab = winner.name),
        DerbyOutcome::Friendly(_) if winners.is_empty() => t!(lang, "derby.nobody-friendly", crab = winner.name),
        DerbyOutcome::Friendly(_) => {
            let paid = |user: &u64| rewards.iter().find(|(u, _)| u == user).map_or(0, |(_, s)| *s);
            t!(
                lang,
                "derby.friendly",
                crab = winner.name,
                list = list(
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
    t!(
        lang,
        "derby.result",
        host = host,
        placings = placings.join(" · "),
        backers = backers
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
        let rows = lobby_buttons(77, Lang::En);
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
        let text = results_text("<@3>", &race, &result, &[], &[10], Lang::En);
        let winner = ROSTER[race.winner()].name;
        assert!(text.contains(&format!("**{winner}**")) && text.contains("||") && text.contains("<@10> +80"));
        assert!(text.contains("pool of **80** shells"));
        assert_eq!(text.matches("||").count(), 2, "one spoiler, opened and closed");
    }
}
