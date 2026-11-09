//! Pearl Flip: an eight by eight Reversi board, with legal squares highlighted.
use crate::App;
use crate::Cx;
use crate::commands::games::board::CHECKERS_SECS;
use crate::commands::games::board::conclude;
use crate::commands::games::board::players;
use crate::commands::games::duel::Kind;
use crate::commands::games::duel::answer;
use crate::commands::games::duel::expire_here;
use crate::commands::games::games::button;
use crate::commands::games::games::pack;
use crate::commands::games::games::quiet;
use crate::commands::games::games::unpack;
use crate::infra::util::BRAND;
use crate::infra::util::SUCCESS;
use crate::infra::util::mention;
use crate::infra::util::relative;
use chrono::{Duration, Utc};
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use pinchora_db::GameSession;
use pinchora_game::play::{Game as Play, quick::Othello};
use pinchora_render::{QuickCard, QuickDetails, QuickKind, Theme};
use serde::{Deserialize, Serialize};
use std::str::Split;
use twilight_model::{
    channel::message::{
        Component,
        component::{ButtonStyle, Label, TextInput, TextInputStyle},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::EmbedBuilder;

#[derive(Serialize, Deserialize)]
struct State {
    pearl: usize,
    game: Othello,
    plies: usize,
    nonce: u64,
    #[serde(default)]
    display_last: Option<usize>,
}
impl State {
    fn mover(&self) -> usize {
        if self.game.turn == 1 {
            self.pearl
        } else {
            1 - self.pearl
        }
    }
}
pub(crate) fn initial_fields() -> String {
    serde_json::to_string(&State {
        pearl: rand::random_range(0..2),
        game: Othello::new(),
        plies: 0,
        nonce: rand::random(),
        display_last: None,
    })
    .expect("othello state")
}
pub(crate) fn slow_players(session: &GameSession) -> Vec<u64> {
    let (fields, _) = unpack(&session.state);
    serde_json::from_str::<State>(fields)
        .ok()
        .map(|s| vec![players(session)[s.mover()]])
        .unwrap_or_default()
}
async fn view(
    app: &App,
    session: &GameSession,
    names: &[String; 2],
    s: &State,
    ended: bool,
    note: &str,
) -> anyhow::Result<InteractionResponseData> {
    let lang = app.guild_lang(session.guild_id as u64).await;
    let legal = s.game.legal(s.game.turn);
    let cells: Vec<String> = s
        .game
        .cells
        .iter()
        .enumerate()
        .map(|(i, &v)| match v {
            1 => "O".to_owned(),
            2 => "X".to_owned(),
            _ if !ended && legal.contains(&i) => "*".to_owned(),
            _ => "·".to_owned(),
        })
        .collect();
    let title = Play::Othello.name(lang);
    let subtitle = t!(lang, "othello.hint");
    let card_names = [names[s.pearl].clone(), names[1 - s.pearl].clone()];
    let turn = (!ended).then_some(s.game.turn - 1);
    let winner = if ended {
        match s.game.score(1).cmp(&s.game.score(2)) {
            std::cmp::Ordering::Greater => Some(0),
            std::cmp::Ordering::Less => Some(1),
            std::cmp::Ordering::Equal => None,
        }
    } else {
        None
    };
    let last = s.display_last;
    let status = if ended {
        t!(lang, "othello.finished")
    } else {
        t!(lang, "othello.turn", name = names[s.mover()])
    };
    let png = app
        .render(move |r| {
            r.quick_card(&QuickCard {
                kind: QuickKind::Othello,
                details: QuickDetails {
                    names: [&card_names[0], &card_names[1]],
                    turn,
                    winner,
                    last,
                    finished: ended,
                    ..Default::default()
                },
                lang,
                title: &title,
                subtitle: &subtitle,
                cells: &cells,
                columns: 8,
                prompt: "",
                status: &status,
                theme: Theme::reef(),
                distort: false,
            })
        })
        .await?;
    let content = if ended {
        t!(
            lang,
            "othello.over",
            score1 = s.game.score(1),
            score2 = s.game.score(2),
            note = note
        )
    } else {
        t!(
            lang,
            "othello.playing",
            mover = mention(players(session)[s.mover()]),
            when = relative(session.expires_at)
        )
    };
    let buttons = if ended {
        vec![]
    } else {
        vec![Component::ActionRow(
            twilight_model::channel::message::component::ActionRow {
                id: None,
                components: vec![button(
                    format!("of:m:{}:{}:{}", session.id, s.plies, s.nonce),
                    &t!(lang, "othello.move"),
                    ButtonStyle::Primary,
                    false,
                )],
            },
        )]
    };
    let mut data = Cx::card(
        EmbedBuilder::new()
            .color(if ended { SUCCESS } else { BRAND })
            .title(Play::Othello.name(lang))
            .description(t!(lang, "othello.hint")),
        "othello.png",
        png,
        buttons,
        false,
    );
    data.content = Some(content);
    data.allowed_mentions = quiet();
    Ok(data)
}
pub(crate) async fn begin(cx: &Cx, session: &GameSession) -> anyhow::Result<()> {
    let (fields, names) = unpack(&session.state);
    let s: State = serde_json::from_str(fields)?;
    cx.update(view(&cx.app, session, &names, &s, false, "").await?).await
}
#[allow(deprecated)]
fn coord_field(lang: Lang) -> Component {
    Component::Label(Label {
        component: Box::new(Component::TextInput(TextInput {
            custom_id: "coord".to_owned(),
            id: None,
            label: None,
            max_length: Some(2),
            min_length: Some(2),
            placeholder: Some("D3".to_owned()),
            required: Some(true),
            style: TextInputStyle::Short,
            value: None,
        })),
        description: None,
        id: None,
        label: t!(lang, "othello.square"),
    })
}
pub(crate) async fn on_component(cx: &Cx, mut parts: Split<'_, char>) -> anyhow::Result<()> {
    let action = parts.next();
    let Some(id) = parts.next().and_then(|s| s.parse::<i64>().ok()) else {
        return cx.reply_ephemeral(t!(cx.lang(), "duel.game-expired")).await;
    };
    match action {
        Some("y") => answer(cx, Kind::Othello, id, true).await,
        Some("n") => answer(cx, Kind::Othello, id, false).await,
        Some("m") => {
            let (plies, nonce) = (parts.next(), parts.next());
            let (Some(plies), Some(nonce)) = (plies, nonce) else {
                return cx.reply_ephemeral(t!(cx.lang(), "duel.menu-expired")).await;
            };
            cx.modal(
                format!("of:f:{id}:{plies}:{nonce}"),
                &Play::Othello.name(cx.lang()),
                vec![coord_field(cx.lang())],
            )
            .await
        }
        _ => cx.reply_ephemeral(t!(cx.lang(), "duel.button-expired")).await,
    }
}
pub(crate) async fn on_modal(cx: &Cx, mut parts: Split<'_, char>, fields: &[(String, String)]) -> anyhow::Result<()> {
    if parts.next() != Some("f") {
        return cx.reply_ephemeral(t!(cx.lang(), "duel.menu-expired")).await;
    }
    let (id, plies, nonce) = (
        parts.next().and_then(|v| v.parse::<i64>().ok()),
        parts.next().and_then(|v| v.parse::<usize>().ok()),
        parts.next().and_then(|v| v.parse::<u64>().ok()),
    );
    let (Some(id), Some(plies), Some(nonce)) = (id, plies, nonce) else {
        return cx.reply_ephemeral(t!(cx.lang(), "duel.menu-expired")).await;
    };
    let Some(session) = cx
        .app
        .db
        .game(id)
        .await?
        .filter(|g| g.is_active() && g.kind == Kind::Othello.key())
    else {
        return cx.reply_ephemeral(t!(cx.lang(), "duel.game-over")).await;
    };
    let me = cx.author()?.id.get();
    if !session.has_player(me) {
        return cx
            .reply_ephemeral(t!(cx.lang(), "duel.not-in-game", command = "othello"))
            .await;
    }
    if session.expired() {
        return expire_here(cx, &session).await;
    }
    let (raw, names) = unpack(&session.state);
    let mut s: State = serde_json::from_str(raw)?;
    if s.plies != plies || s.nonce != nonce {
        return cx.reply_ephemeral(t!(cx.lang(), "duel.menu-expired")).await;
    }
    if players(&session)[s.mover()] != me {
        return cx.reply_ephemeral(t!(cx.lang(), "duel.not-your-turn")).await;
    }
    let coord = fields.iter().find(|(k, _)| k == "coord").map_or("", |(_, v)| v.trim());
    let bytes = coord.as_bytes();
    let cell = if bytes.len() == 2 {
        let col = bytes[0].to_ascii_uppercase().wrapping_sub(b'A');
        let row = bytes[1].wrapping_sub(b'1');
        if col < 8 && row < 8 {
            Some(row as usize * 8 + col as usize)
        } else {
            None
        }
    } else {
        None
    };
    let Some(cell) = cell else {
        return cx.reply_ephemeral(t!(cx.lang(), "board.illegal")).await;
    };
    if !s.game.play(cell) {
        return cx.reply_ephemeral(t!(cx.lang(), "board.illegal")).await;
    }
    s.display_last = Some(cell);
    s.plies += 1;
    s.nonce = rand::random();
    let done = s.game.over();
    let packed = pack(&serde_json::to_string(&s)?, [&names[0], &names[1]]);
    if done {
        let winner = match s.game.score(1).cmp(&s.game.score(2)) {
            std::cmp::Ordering::Greater => Some(players(&session)[s.pearl]),
            std::cmp::Ordering::Less => Some(players(&session)[1 - s.pearl]),
            std::cmp::Ordering::Equal => None,
        };
        let Some(note) = conclude(
            &cx.app,
            cx.app.guild_lang(session.guild_id as u64).await,
            &session,
            &packed,
            Play::Othello,
            winner,
            s.plies,
        )
        .await?
        else {
            return cx.reply_ephemeral(t!(cx.lang(), "duel.board-changed")).await;
        };
        cx.update(view(&cx.app, &session, &names, &s, true, &note).await?).await
    } else {
        if !cx
            .app
            .db
            .advance_game(id, &session.state, &packed, CHECKERS_SECS)
            .await?
        {
            return cx.reply_ephemeral(t!(cx.lang(), "duel.board-changed")).await;
        }
        let mut next = session;
        next.expires_at = Utc::now() + Duration::seconds(CHECKERS_SECS);
        cx.update(view(&cx.app, &next, &names, &s, false, "").await?).await
    }
}
