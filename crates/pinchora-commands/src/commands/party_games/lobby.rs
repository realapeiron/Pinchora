//! The lobby the group games share: Join, Leave, Start and Cancel on one message.

use std::sync::Arc;

use pinchora_core::{i18n::Lang, t};
use pinchora_db::{Commit, LOBBY, NewParty, PartyGame, PartyJoin, PartyLeave, PartyPlayer};
use pinchora_game::{lang::upper, party::Party};
use pinchora_render::PartyLobbyCard;
use twilight_model::{channel::message::component::ButtonStyle, http::interaction::InteractionResponseData};
use twilight_util::builder::embed::EmbedBuilder;

use super::{
    after, card, custom_id, dare, edit_card, game_lang, names, pearl, push_button, race, remember, row, snapshot, text,
    theme, track_message, writing,
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

/// What a lobby tells players about its game: a line about it, and the rules in a few words.
fn blurb(lang: Lang, party: Party, game: &PartyGame) -> (String, String) {
    let (min, max) = party.players();
    let table = t!(lang, "party.players-range", min = min, max = max);
    match party {
        Party::HotPearl => (
            t!(lang, "hotpearl.blurb"),
            t!(lang, "hotpearl.rules", table = table, count = game.rounds),
        ),
        Party::Acronym => (
            t!(lang, "acronym.blurb"),
            t!(lang, "acronym.rules", table = table, rounds = game.rounds),
        ),
        Party::Postcard => (
            t!(lang, "postcard.blurb"),
            t!(lang, "postcard.rules", table = table, rounds = game.rounds),
        ),
        Party::WordRace => (t!(lang, "wordrace.blurb"), t!(lang, "wordrace.rules", table = table)),
        Party::TruthOrDare => (t!(lang, "dare.blurb"), t!(lang, "dare.rules", table = table)),
        Party::WouldYouRather => (String::new(), String::new()),
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
        let lang = cx.lang();
        let what = Party::from_key(&busy.kind).map_or_else(|| t!(lang, "party.a-party-game"), |p| p.label_in(lang));
        return cx.reply_ephemeral(t!(lang, "party.busy", what = what)).await;
    }
    let host = cx.author()?.id.get();
    let game = app
        .db
        .create_party(NewParty {
            kind: party.key(),
            guild_id: cx.guild_id().unwrap_or(0),
            channel_id: channel,
            host,
            host_name: &text::name(&cx.author_display_name(), cx.lang()),
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
    let lang = game_lang(game);
    let (min, max) = party.players();
    let (tagline, rules) = blurb(lang, party, game);
    let shown = names(players);
    let label = party.label_in(lang);
    let png = {
        // The card draws its title and footer in capitals, which Turkish spells with its own İ.
        let (title, tagline, shown) = (upper(lang, &label), tagline.clone(), shown.clone());
        let footer = upper(lang, &t!(lang, "party.lobby.ready"));
        app.render(move |r| {
            r.party_lobby_card(&PartyLobbyCard {
                lang,
                title: &title,
                tagline: &tagline,
                players: &shown,
                min,
                max,
                footer: &footer,
                theme: theme(),
            })
        })
        .await?
    };
    let id = game.id;
    let enough = players.len() >= min;
    let components = vec![row(vec![
        push_button(party, &[&"j", &id], &t!(lang, "party.btn.join"), ButtonStyle::Primary),
        push_button(
            party,
            &[&"l", &id],
            &t!(lang, "party.btn.leave"),
            ButtonStyle::Secondary,
        ),
        crate::commands::games::button(
            custom_id(party, &[&"s", &id]),
            &t!(lang, "party.btn.start"),
            if enough {
                ButtonStyle::Success
            } else {
                ButtonStyle::Secondary
            },
            false,
        ),
        push_button(party, &[&"x", &id], &t!(lang, "party.btn.cancel"), ButtonStyle::Danger),
    ])];
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(format!("{} {}", party.emoji(), label))
        .description(t!(
            lang,
            "party.lobby.description",
            tagline = tagline,
            rules = rules,
            host = mention(game.host()),
            when = relative(game.deadline)
        ));
    Ok(card(
        Some(t!(
            lang,
            "party.lobby.opened",
            emoji = party.emoji(),
            host = mention(game.host()),
            label = label,
            n = players.len(),
            max = max
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
        cx.reply_ephemeral(t!(cx.lang(), "party.lobby.closed")).await?;
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
        return cx.reply_ephemeral(t!(cx.lang(), "party.lobby.expired")).await;
    };
    remember(cx, &game).await;
    cx.update(view(&cx.app, &game, &players).await?).await
}

async fn join(cx: &Cx, party: Party, id: i64) -> anyhow::Result<()> {
    let user = cx.author()?.id.get();
    let name = text::name(&cx.author_display_name(), cx.lang());
    match cx
        .app
        .db
        .join_party(id, user, &name, party.players().1, cx.app.now())
        .await?
    {
        PartyJoin::Joined => refresh(cx, party, id).await,
        PartyJoin::Already => cx.reply_ephemeral(t!(cx.lang(), "party.lobby.already-in")).await,
        PartyJoin::Full => cx.reply_ephemeral(t!(cx.lang(), "party.lobby.full")).await,
        PartyJoin::Closed => cx.reply_ephemeral(t!(cx.lang(), "party.lobby.closed")).await,
    }
}

async fn leave(cx: &Cx, party: Party, id: i64) -> anyhow::Result<()> {
    let user = cx.author()?.id.get();
    match cx.app.db.leave_party(id, user, cx.app.now()).await? {
        PartyLeave::Left => refresh(cx, party, id).await,
        PartyLeave::NotIn => cx.reply_ephemeral(t!(cx.lang(), "party.lobby.not-in")).await,
        PartyLeave::Host => cx.reply_ephemeral(t!(cx.lang(), "party.lobby.host-leaves")).await,
        PartyLeave::Closed => cx.reply_ephemeral(t!(cx.lang(), "party.lobby.closed")).await,
    }
}

async fn cancel(cx: &Cx, id: i64) -> anyhow::Result<()> {
    let user = cx.author()?.id.get();
    let Some(game) = cx.app.db.party(id).await? else {
        return cx.reply_ephemeral(t!(cx.lang(), "party.lobby.expired")).await;
    };
    if user != game.host() && !cx.can_manage_guild() {
        return cx
            .reply_ephemeral(t!(
                cx.lang(),
                "party.lobby.only-host-cancels",
                host = mention(game.host())
            ))
            .await;
    }
    if !game.in_lobby() {
        return cx.reply_ephemeral(t!(cx.lang(), "party.lobby.started")).await;
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
        return cx.reply_ephemeral(t!(cx.lang(), "party.lobby.changed")).await;
    }
    remember(cx, &game).await;
    cx.update(closed(t!(
        game_lang(&game),
        "party.lobby.cancelled",
        user = mention(user)
    )))
    .await
}

async fn start(cx: &Cx, party: Party, id: i64) -> anyhow::Result<()> {
    let user = cx.author()?.id.get();
    for _ in 0..3 {
        let Some((game, players)) = snapshot(&cx.app, party, id).await? else {
            return cx.reply_ephemeral(t!(cx.lang(), "party.lobby.expired")).await;
        };
        if !game.in_lobby() || game.late(cx.app.now()) {
            return cx.reply_ephemeral(t!(cx.lang(), "party.lobby.closed")).await;
        }
        if user != game.host() {
            return cx
                .reply_ephemeral(t!(
                    cx.lang(),
                    "party.lobby.only-host-starts",
                    host = mention(game.host())
                ))
                .await;
        }
        let (min, _) = party.players();
        if players.len() < min {
            let lang = cx.lang();
            return cx
                .reply_ephemeral(t!(
                    lang,
                    "party.lobby.too-few",
                    label = party.label_in(lang),
                    min = min,
                    n = players.len()
                ))
                .await;
        }
        remember(cx, &game).await;
        let started = match party {
            Party::HotPearl => pearl::start(cx, &game, &players).await?,
            Party::Acronym | Party::Postcard => writing::start(cx, &game, &players).await?,
            Party::WordRace => race::start(cx, &game, &players).await?,
            Party::TruthOrDare => dare::start(cx, &game, &players).await?,
            Party::WouldYouRather => return cx.reply_ephemeral(t!(cx.lang(), "party.lobby.none")).await,
        };
        if matches!(started, Started::Yes) {
            return Ok(());
        }
    }
    cx.reply_ephemeral(t!(cx.lang(), "party.lobby.kept-changing")).await
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
        &closed(t!(game_lang(&done.game), "party.lobby.timed-out")),
    )
    .await;
    Ok(true)
}
