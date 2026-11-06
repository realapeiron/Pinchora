//! `/play 2048`: Tide 2048. Slide the sea creatures with the arrow buttons; two of a kind merge into the
//! next one up. One game per player, kept in the database and picked up where it was left. A finished
//! game counts in `/play stats` with the player's best score, and pays a few shells for a big tile.

use std::{str::Split, sync::OnceLock};

use pinchora_core::{i18n::Lang, t};
use pinchora_db::{SoloGame, SoloStep};
use pinchora_game::play::{
    Game,
    tide2048::{Board, Dir, GOAL, reward, value},
};
use pinchora_render::{SpriteArt, Theme, Tide2048Card};
use twilight_model::{
    channel::message::{
        Component,
        component::{ActionRow, ButtonStyle},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::EmbedBuilder;

use crate::{
    App, Cx,
    commands::games::{button, pay_win},
    game::scene::scaled_png,
    util::{BRAND, SUCCESS},
};

const GAME: &str = "tide2048";

/// Moves before a game may be ended by hand, so ending is not a way to count games that were never played.
const MIN_MOVES_TO_END: i32 = 5;

/// The creature on each tile, from the "2" up: the fish atlas's id. What the tile is called is
/// `tide.creature.<index>` in the catalogs.
const CREATURES: [&str; 13] = [
    "pebblepin_shrimp",
    "mintcap_limpet",
    "sandloop_cockle",
    "toaststripe_minnow",
    "mossbutton_perch",
    "plumspot_puffer",
    "reefruff_lionfish",
    "mossmosaic_lobster",
    "chartcurl_nautilus",
    "petalwake_manta",
    "aurorawoven_leviathan",
    "dawncrown_coelacanth",
    "moonkeel_chest",
];

/// The creatures' pictures, scaled once for the whole process.
fn art(app: &App) -> &'static [(Vec<u8>, u32, u32)] {
    static ART: OnceLock<Vec<(Vec<u8>, u32, u32)>> = OnceLock::new();
    ART.get_or_init(|| {
        CREATURES
            .iter()
            .map(|id| scaled_png(&app.sprites.fish(id), 3).unwrap_or_else(|_| (Vec::new(), 0, 0)))
            .collect()
    })
}

/// What the biggest tile is called; bigger tiles than the list keep the last creature.
fn creature(lang: Lang, exponent: u8) -> String {
    let index = usize::from(exponent).saturating_sub(1).min(CREATURES.len() - 1);
    t!(lang, &format!("tide.creature.{index}"))
}

/// A new tile for the board: usually a 2, sometimes a 4, anywhere free. Returns where it landed.
fn deal(board: &mut Board) -> Option<usize> {
    board.spawn(rand::random_range(0..16usize), rand::random_range(0..10u8) == 0)
}

fn new_board() -> Board {
    let mut board = Board::new();
    deal(&mut board);
    deal(&mut board);
    board
}

/// `/play 2048`: the player's game, or a new one when they have none or it is finished.
pub(crate) async fn start(cx: &Cx) -> anyhow::Result<()> {
    let me = cx.author()?.id.get();
    let game = match cx.app.db.solo_game(me, GAME).await? {
        Some(game) if !game.over() => game,
        _ => cx.app.db.start_solo(me, GAME, &new_board().encode(), true).await?,
    };
    let data = view(&cx.app, cx.lang(), me, &game, None, "").await?;
    cx.reply(data).await
}

/// The private board: the picture, the arrows and, once the game is over, the way to start another.
async fn view(
    app: &App,
    lang: Lang,
    me: u64,
    game: &SoloGame,
    fresh: Option<usize>,
    note: &str,
) -> anyhow::Result<InteractionResponseData> {
    let board = Board::decode(&game.state).ok_or_else(|| anyhow::anyhow!("bad tide 2048 state {:?}", game.state))?;
    let stats = app.db.game_stats(me).await?;
    let saved = stats.iter().find(|s| s.game == GAME).map_or(0, |s| s.best);
    let (score, best) = (game.score.max(0) as u32, game.score.max(saved).max(0) as u32);
    let biggest = board.best();
    let biggest_name = creature(lang, biggest);
    let status = if game.over() {
        t!(lang, "tide.status-over", creature = biggest_name)
    } else {
        t!(lang, "tide.status-move", moves = game.moves, creature = biggest_name)
    };
    let cells = *board.cells();
    let png = {
        let status = pinchora_game::lang::upper(lang, &status);
        let art = art(app);
        app.render(move |r| {
            let creatures: Vec<SpriteArt<'_>> = art.iter().map(|(png, w, h)| SpriteArt { png, w: *w, h: *h }).collect();
            r.tide2048_card(&Tide2048Card {
                cells: &cells,
                score,
                best,
                creatures: &creatures,
                fresh,
                status: &status,
                lang,
                theme: Theme::reef(),
            })
        })
        .await?
    };
    let description = if game.over() {
        t!(
            lang,
            "tide.final",
            score = score,
            value = value(biggest),
            creature = biggest_name,
            note = note
        )
    } else {
        t!(lang, "tide.how-to", goal = creature(lang, GOAL), note = note)
    };
    let embed = EmbedBuilder::new()
        .color(if game.over() { SUCCESS } else { BRAND })
        .title(t!(lang, "game.tide2048"))
        .description(description);
    let components = if game.over() {
        vec![row(vec![button(
            "tz:n".to_owned(),
            &t!(lang, "tide.new-game"),
            ButtonStyle::Primary,
            false,
        )])]
    } else {
        let arrow = |dir: Dir, label: &str| {
            button(
                format!("tz:m:{}:{}", dir.key(), game.moves),
                label,
                ButtonStyle::Secondary,
                board.slide(dir).is_none(),
            )
        };
        vec![row(vec![
            arrow(Dir::Left, "←"),
            arrow(Dir::Up, "↑"),
            arrow(Dir::Down, "↓"),
            arrow(Dir::Right, "→"),
            button(
                format!("tz:e:{}", game.moves),
                &t!(lang, "tide.finish"),
                ButtonStyle::Success,
                game.moves < MIN_MOVES_TO_END,
            ),
        ])]
    };
    Ok(Cx::card(embed, "tide2048.png", png, components, true))
}

fn row(components: Vec<Component>) -> Component {
    Component::ActionRow(ActionRow { id: None, components })
}

/// `tz:m:<direction>:<moves>` slides, `tz:e:<moves>` finishes the game and `tz:n` starts another.
pub(crate) async fn on_button(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let me = cx.author()?.id.get();
    let lang = cx.lang();
    let action = args.next();
    if action == Some("n") {
        let game = match cx.app.db.solo_game(me, GAME).await? {
            Some(game) if !game.over() => game,
            _ => cx.app.db.start_solo(me, GAME, &new_board().encode(), true).await?,
        };
        let data = view(&cx.app, lang, me, &game, None, "").await?;
        return cx.update(data).await;
    }
    let Some(game) = cx.app.db.solo_game(me, GAME).await? else {
        return cx.reply_ephemeral(t!(lang, "tide.no-game")).await;
    };
    let (dir, from_moves) = match action {
        Some("m") => (
            args.next().and_then(|d| d.chars().next()).and_then(Dir::from_key),
            args.next().and_then(|v| v.parse::<i32>().ok()),
        ),
        Some("e") => (None, args.next().and_then(|v| v.parse::<i32>().ok())),
        _ => return cx.reply_ephemeral(t!(lang, "tide.button-expired")).await,
    };
    let Some(from_moves) = from_moves else {
        return cx.reply_ephemeral(t!(lang, "tide.button-expired")).await;
    };
    // A click from a board that has moved on, or from a finished game, just shows the board as it is.
    if game.over() || game.moves != from_moves {
        let data = view(&cx.app, lang, me, &game, None, "").await?;
        return cx.update(data).await;
    }
    let board = Board::decode(&game.state).ok_or_else(|| anyhow::anyhow!("bad tide 2048 state {:?}", game.state))?;
    let (next, score, over, fresh) = match dir {
        Some(dir) => {
            let Some((mut next, gained)) = board.slide(dir) else {
                return cx.reply_ephemeral(t!(lang, "tide.nothing-moves")).await;
            };
            let fresh = deal(&mut next);
            let over = !next.can_move();
            (next, game.score + i32::try_from(gained).unwrap_or(0), over, fresh)
        }
        None => {
            if game.moves < MIN_MOVES_TO_END {
                return cx.reply_ephemeral(t!(lang, "tide.more-moves")).await;
            }
            (board, game.score, true, None)
        }
    };
    let step = SoloStep {
        state: &next.encode(),
        score,
        over,
    };
    if !cx.app.db.advance_solo(me, GAME, game.moves, &step).await? {
        // Another click moved first.
        let current = cx.app.db.solo_game(me, GAME).await?.unwrap_or(game);
        let data = view(&cx.app, lang, me, &current, None, "").await?;
        return cx.update(data).await;
    }
    let note = if over {
        finish(&cx.app, lang, me, &next, score).await?
    } else {
        String::new()
    };
    let updated = SoloGame {
        state: next.encode(),
        score,
        moves: game.moves + 1,
        status: if over { "over" } else { "playing" }.to_owned(),
        ..game
    };
    let data = view(&cx.app, lang, me, &updated, fresh, &note).await?;
    cx.update(data).await
}

/// Counts a finished game and pays for the biggest tile (limited per day); returns what to tell the player.
async fn finish(app: &App, lang: Lang, me: u64, board: &Board, score: i32) -> anyhow::Result<String> {
    let best = board.best();
    if let Err(error) = app.db.record_game_score(me, GAME, best >= GOAL, score).await {
        tracing::warn!(%error, me, "game stats not saved");
    }
    let shells = reward(best);
    if shells == 0 {
        return Ok(t!(lang, "tide.reach-256"));
    }
    let paid = pay_win(app, me, Game::Tide2048, shells, "tide2048_game").await?;
    Ok(if paid > 0 {
        t!(lang, "tide.paid", paid = paid)
    } else {
        t!(lang, "tide.limit-reached")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_creature_has_a_sprite_and_a_name() {
        let sprites = pinchora_render::sprites::SpriteStore::embedded();
        for (index, id) in CREATURES.iter().enumerate() {
            assert!(sprites.get(&format!("c_{id}")).is_some(), "{id} is not in the atlas");
            for lang in Lang::ALL {
                let name = t!(lang, &format!("tide.creature.{index}"));
                assert!(
                    !name.is_empty() && !name.starts_with("tide.creature."),
                    "{id} in {lang:?}"
                );
            }
        }
        assert_eq!(creature(Lang::En, 1), "Shrimp");
        assert_eq!(creature(Lang::En, GOAL), "Leviathan");
        assert_eq!(creature(Lang::Tr, 1), "Karides");
        assert_eq!(
            creature(Lang::En, 30),
            "Sunken chest",
            "bigger tiles keep the last creature"
        );
    }

    #[test]
    fn a_new_board_has_two_tiles() {
        let board = new_board();
        assert_eq!(board.empties(), 14);
        assert!(board.best() <= 2);
    }
}
