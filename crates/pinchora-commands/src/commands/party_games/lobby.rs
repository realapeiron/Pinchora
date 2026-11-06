//! The lobby the group games share: Join, Leave, Start and Cancel on one message.

use std::sync::Arc;

use pinchora_db::{Commit, LOBBY, NewParty, PartyGame, PartyJoin, PartyLeave, PartyPlayer};
use pinchora_game::party::Party;
use pinchora_render::PartyLobbyCard;
use twilight_model::{channel::message::component::ButtonStyle, http::interaction::InteractionResponseData};
use twilight_util::builder::embed::EmbedBuilder;

use super::{
    after, card, custom_id, dare, edit_card, names, pearl, push_button, race, remember, row, snapshot, text, theme,
    track_message, writing,
};
use crate::{
    App, Cx,
    commands::games::{channel_of, quiet},
    util::{BRAND, mention, relative},
};

/// How a game's `start` ended.
pub(crate) enum Started {
    /// The game began and the message was updated.
    Yes,
    /// Somebody joined or left in between: look at the lobby again.
    Stale,
}

/// What a lobby tells players about its game.
fn blurb(party: Party, game: &PartyGame) -> (&'static str, String) {
    let (min, max) = party.players();
    let table = format!("{min}-{max} players");
    match party {
        Party::HotPearl => (
            "A syllable appears. Type a word that contains it before the pearl pops!",
            format!(
                "{table} · {} li{} each · every pop costs a life · the last player with lives wins",
                game.rounds,
                if game.rounds == 1 { "fe" } else { "ves" }
            ),
        ),
        Party::Acronym => (
            "Letters appear. Write a phrase whose words start with them in order, then vote for the best one.",
            format!(
                "{table} · {} rounds · votes are anonymous until the reveal",
                game.rounds
            ),
        ),
        Party::Postcard => (
            "A pixel postcard appears. Write the funniest caption, then vote for your favourite.",
            format!(
                "{table} · {} rounds · votes are anonymous until the reveal",
                game.rounds
            ),
        ),
        Party::WordRace => (
            "Everyone races to find the same five-letter word on a private board. First to solve it wins.",
            format!("{table} · six guesses each · you only see colours on the other boards"),
        ),
        Party::TruthOrDare => (
            "Take turns: a random player picks truth or dare. Everything here is family friendly.",
            format!("{table} · the host ends the game when everybody has had enough"),
        ),
        Party::WouldYouRather => ("", String::new()),
    }
}

/// A message that closes a lobby or a game: text only, no picture, no buttons.
pub(crate) fn closed(text: impl Into<String>) -> InteractionResponseData {
    InteractionResponseData {
        content: Some(text.into()),
        embeds: Some(Vec::new()),
        components: Some(Vec::new()),
        attachments: Some(Vec::new()),
        allowed_mentions: quiet(),
        ..Default::default()
    }
}

/// Opens a lobby for `party` in this channel, with `state` as the game's own settings.
pub(crate) async fn open(cx: &Cx, party: Party, rounds: i32, state: String) -> anyhow::Result<()> {
    let app = &cx.app;
    let channel = channel_of(cx);
    if let Some(busy) = app.db.party_in_channel(channel, app.now()).await? {
        let what = Party::from_key(&busy.kind).map_or("A party game", Party::label);
        return cx
            .reply_ephemeral(format!("{what} is already going in this channel. Let it finish first!"))
            .await;
    }
    let host = cx.author()?.id.get();
    let game = app
        .db
        .create_party(NewParty {
            kind: party.key(),
            guild_id: cx.guild_id().unwrap_or(0),
            channel_id: channel,
            host,
            host_name: &text::name(&cx.author_display_name()),
            rounds,
            status: LOBBY,
            phase: "lobby",
            state: &state,
            deadline: after(app, party.lobby_secs()),
        })
        .await?;
    let players = app.db.party_players(game.id).await?;
    cx.reply(view(app, &game, &players).await?).await?;
    track_message(cx, game.id);
    Ok(())
}

/// The lobby message: the card with its seats, who hosts, and the four buttons.
pub(crate) async fn view(
    app: &App,
    game: &PartyGame,
    players: &[PartyPlayer],
) -> anyhow::Result<InteractionResponseData> {
    let party = Party::from_key(&game.kind).ok_or_else(|| anyhow::anyhow!("lobby of an unknown game"))?;
    let (min, max) = party.players();
    let (tagline, rules) = blurb(party, game);
    let shown = names(players);
    let png = {
        let (title, tagline, shown) = (party.label().to_owned(), tagline.to_owned(), shown.clone());
        app.render(move |r| {
            r.party_lobby_card(&PartyLobbyCard {
                title: &title,
                tagline: &tagline,
                players: &shown,
                min,
                max,
                footer: "Ready: the host can press start",
                theme: theme(),
            })
        })
        .await?
    };
    let id = game.id;
    let enough = players.len() >= min;
    let components = vec![row(vec![
        push_button(party, &[&"j", &id], "Join", ButtonStyle::Primary),
        push_button(party, &[&"l", &id], "Leave", ButtonStyle::Secondary),
        crate::commands::games::button(
            custom_id(party, &[&"s", &id]),
            "Start",
            if enough {
                ButtonStyle::Success
            } else {
                ButtonStyle::Secondary
            },
            false,
        ),
        push_button(party, &[&"x", &id], "Cancel", ButtonStyle::Danger),
    ])];
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(format!("{} {}", party.emoji(), party.label()))
        .description(format!(
            "{tagline}\n\n{rules}\nHost: {} · closes {}",
            mention(game.host()),
            relative(game.deadline)
        ));
    Ok(card(
        Some(format!(
            "{} {} opened a **{}** lobby! Press **Join** ({}/{max}).",
            party.emoji(),
            mention(game.host()),
            party.label(),
            players.len()
        )),
        embed,
        "lobby.png",
        png,
        components,
    ))
}

/// Handles the lobby's four buttons; false when `action` is something else, for the game to handle.
pub(crate) async fn on_button(cx: &Cx, party: Party, action: &str, id: i64) -> anyhow::Result<bool> {
    if !matches!(action, "j" | "l" | "s" | "x") {
        return Ok(false);
    }
    // Another game's button with this lobby's id (a forged custom id) would start that game here or seat
    // more players than this one takes.
    if cx.app.db.party(id).await?.is_some_and(|game| game.kind != party.key()) {
        cx.reply_ephemeral("This lobby has closed.").await?;
        return Ok(true);
    }
    match action {
        "j" => join(cx, party, id).await?,
        "l" => leave(cx, party, id).await?,
        "x" => cancel(cx, id).await?,
        _ => start(cx, party, id).await?,
    }
    Ok(true)
}

async fn refresh(cx: &Cx, party: Party, id: i64) -> anyhow::Result<()> {
    let Some((game, players)) = snapshot(&cx.app, party, id).await? else {
        return cx.reply_ephemeral("This lobby has expired.").await;
    };
    remember(cx, &game).await;
    cx.update(view(&cx.app, &game, &players).await?).await
}

async fn join(cx: &Cx, party: Party, id: i64) -> anyhow::Result<()> {
    let user = cx.author()?.id.get();
    let name = text::name(&cx.author_display_name());
    match cx
        .app
        .db
        .join_party(id, user, &name, party.players().1, cx.app.now())
        .await?
    {
        PartyJoin::Joined => refresh(cx, party, id).await,
        PartyJoin::Already => {
            cx.reply_ephemeral("You are already in. Wait for the host to press Start!")
                .await
        }
        PartyJoin::Full => cx.reply_ephemeral("This lobby is full.").await,
        PartyJoin::Closed => cx.reply_ephemeral("This lobby has closed.").await,
    }
}

async fn leave(cx: &Cx, party: Party, id: i64) -> anyhow::Result<()> {
    let user = cx.author()?.id.get();
    match cx.app.db.leave_party(id, user, cx.app.now()).await? {
        PartyLeave::Left => refresh(cx, party, id).await,
        PartyLeave::NotIn => cx.reply_ephemeral("You are not in this lobby.").await,
        PartyLeave::Host => {
            cx.reply_ephemeral("You are the host: press Cancel to close the lobby instead.")
                .await
        }
        PartyLeave::Closed => cx.reply_ephemeral("This lobby has closed.").await,
    }
}

async fn cancel(cx: &Cx, id: i64) -> anyhow::Result<()> {
    let user = cx.author()?.id.get();
    let Some(game) = cx.app.db.party(id).await? else {
        return cx.reply_ephemeral("This lobby has expired.").await;
    };
    if user != game.host() && !cx.can_manage_guild() {
        return cx
            .reply_ephemeral(format!("Only {} can cancel this lobby.", mention(game.host())))
            .await;
    }
    if !game.in_lobby() {
        return cx.reply_ephemeral("This game has already started.").await;
    }
    let commit = Commit {
        phase: "done",
        round: game.round,
        state: &game.state,
        deadline: game.deadline,
        status: "done",
        players: &[],
        entries: &[],
        pay: None,
    };
    if cx.app.db.commit_party(id, game.seq, commit).await?.is_none() {
        return cx
            .reply_ephemeral("Somebody changed the lobby just now. Try again!")
            .await;
    }
    remember(cx, &game).await;
    cx.update(closed(format!("🚫 {} cancelled the lobby.", mention(user))))
        .await
}

async fn start(cx: &Cx, party: Party, id: i64) -> anyhow::Result<()> {
    let user = cx.author()?.id.get();
    for _ in 0..3 {
        let Some((game, players)) = snapshot(&cx.app, party, id).await? else {
            return cx.reply_ephemeral("This lobby has expired.").await;
        };
        if !game.in_lobby() || game.late(cx.app.now()) {
            return cx.reply_ephemeral("This lobby has closed.").await;
        }
        if user != game.host() {
            return cx
                .reply_ephemeral(format!("Only {} can start this game.", mention(game.host())))
                .await;
        }
        let (min, _) = party.players();
        if players.len() < min {
            return cx
                .reply_ephemeral(format!(
                    "{} needs at least {min} players, and only {} joined so far.",
                    party.label(),
                    players.len()
                ))
                .await;
        }
        remember(cx, &game).await;
        let started = match party {
            Party::HotPearl => pearl::start(cx, &game, &players).await?,
            Party::Acronym | Party::Postcard => writing::start(cx, &game, &players).await?,
            Party::WordRace => race::start(cx, &game, &players).await?,
            Party::TruthOrDare => dare::start(cx, &game, &players).await?,
            Party::WouldYouRather => return cx.reply_ephemeral("This one has no lobby.").await,
        };
        if matches!(started, Started::Yes) {
            return Ok(());
        }
    }
    cx.reply_ephemeral("The lobby kept changing. Press Start again in a moment!")
        .await
}

/// Closes a lobby nobody started in time.
pub(crate) async fn expire(app: &Arc<App>, game: PartyGame) -> anyhow::Result<bool> {
    let commit = Commit {
        phase: "done",
        round: game.round,
        state: &game.state,
        deadline: game.deadline,
        status: "done",
        players: &[],
        entries: &[],
        pay: None,
    };
    let Some(done) = app.db.commit_party(game.id, game.seq, commit).await? else {
        return Ok(false);
    };
    edit_card(
        app,
        &done.game,
        &closed("⌛ Nobody started this lobby in time, so it was closed."),
    )
    .await;
    Ok(true)
}
