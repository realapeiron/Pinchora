//! `/play 2048`: Tide 2048. Slide the sea creatures with the arrow buttons; two of a kind merge into the
//! next one up. One game per player, kept in the database and picked up where it was left. A finished
//! game counts in `/play stats` with the player's best score, and pays a few shells for a big tile.

use std::{str::Split, sync::OnceLock};

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

/// The creature on each tile, from the "2" up: the fish atlas's id and what the tile is called.
const CREATURES: [(&str, &str); 13] = [
    ("pebblepin_shrimp", "Shrimp"),
    ("mintcap_limpet", "Limpet"),
    ("sandloop_cockle", "Scallop"),
    ("toaststripe_minnow", "Minnow"),
    ("mossbutton_perch", "Perch"),
    ("plumspot_puffer", "Puffer"),
    ("reefruff_lionfish", "Lionfish"),
    ("mossmosaic_lobster", "Lobster"),
    ("chartcurl_nautilus", "Nautilus"),
    ("petalwake_manta", "Manta"),
    ("aurorawoven_leviathan", "Leviathan"),
    ("dawncrown_coelacanth", "Coelacanth"),
    ("moonkeel_chest", "Sunken chest"),
];

/// The creatures' pictures, scaled once for the whole process.
fn art(app: &App) -> &'static [(Vec<u8>, u32, u32)] {
    static ART: OnceLock<Vec<(Vec<u8>, u32, u32)>> = OnceLock::new();
    ART.get_or_init(|| {
        CREATURES
            .iter()
            .map(|(id, _)| scaled_png(&app.sprites.fish(id), 3).unwrap_or_else(|_| (Vec::new(), 0, 0)))
            .collect()
    })
}

/// What the biggest tile is called.
fn creature(exponent: u8) -> &'static str {
    CREATURES
        .get(usize::from(exponent).saturating_sub(1))
        .map_or(CREATURES[CREATURES.len() - 1].1, |c| c.1)
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
    let data = view(&cx.app, me, &game, None, "").await?;
    cx.reply(data).await
}

/// The private board: the picture, the arrows and, once the game is over, the way to start another.
async fn view(
    app: &App,
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
    let status = if game.over() {
        format!("Game over! Biggest: {}", creature(biggest))
    } else {
        format!("Move {} · biggest: {}", game.moves, creature(biggest))
    };
    let cells = *board.cells();
    let png = {
        let status = status.clone();
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
                theme: Theme::reef(),
            })
        })
        .await?
    };
    let description = if game.over() {
        format!(
            "Final score **{score}**, biggest tile **{}** ({}). {note}",
            value(biggest),
            creature(biggest)
        )
    } else {
        format!(
            "Slide with the arrows: two creatures of a kind merge into the next one up. Reach the **{}** (2048) to win, then keep going. {note}",
            creature(GOAL)
        )
    };
    let embed = EmbedBuilder::new()
        .color(if game.over() { SUCCESS } else { BRAND })
        .title("Tide 2048")
        .description(description);
    let components = if game.over() {
        vec![row(vec![button(
            "tz:n".to_owned(),
            "New game",
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
                "Finish",
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
    let action = args.next();
    if action == Some("n") {
        let game = match cx.app.db.solo_game(me, GAME).await? {
            Some(game) if !game.over() => game,
            _ => cx.app.db.start_solo(me, GAME, &new_board().encode(), true).await?,
        };
        let data = view(&cx.app, me, &game, None, "").await?;
        return cx.update(data).await;
    }
    let Some(game) = cx.app.db.solo_game(me, GAME).await? else {
        return cx
            .reply_ephemeral("You have no game. Start one with `/play 2048`!")
            .await;
    };
    let (dir, from_moves) = match action {
        Some("m") => (
            args.next().and_then(|d| d.chars().next()).and_then(Dir::from_key),
            args.next().and_then(|v| v.parse::<i32>().ok()),
        ),
        Some("e") => (None, args.next().and_then(|v| v.parse::<i32>().ok())),
        _ => return cx.reply_ephemeral("This button has expired.").await,
    };
    let Some(from_moves) = from_moves else {
        return cx.reply_ephemeral("This button has expired.").await;
    };
    // A click from a board that has moved on, or from a finished game, just shows the board as it is.
    if game.over() || game.moves != from_moves {
        let data = view(&cx.app, me, &game, None, "").await?;
        return cx.update(data).await;
    }
    let board = Board::decode(&game.state).ok_or_else(|| anyhow::anyhow!("bad tide 2048 state {:?}", game.state))?;
    let (next, score, over, fresh) = match dir {
        Some(dir) => {
            let Some((mut next, gained)) = board.slide(dir) else {
                return cx.reply_ephemeral("Nothing moves that way.").await;
            };
            let fresh = deal(&mut next);
            let over = !next.can_move();
            (next, game.score + i32::try_from(gained).unwrap_or(0), over, fresh)
        }
        None => {
            if game.moves < MIN_MOVES_TO_END {
                return cx.reply_ephemeral("Make a few more moves first.").await;
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
        let data = view(&cx.app, me, &current, None, "").await?;
        return cx.update(data).await;
    }
    let note = if over {
        finish(&cx.app, me, &next, score).await?
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
    let data = view(&cx.app, me, &updated, fresh, &note).await?;
    cx.update(data).await
}

/// Counts a finished game and pays for the biggest tile (limited per day); returns what to tell the player.
async fn finish(app: &App, me: u64, board: &Board, score: i32) -> anyhow::Result<String> {
    let best = board.best();
    if let Err(error) = app.db.record_game_score(me, GAME, best >= GOAL, score).await {
        tracing::warn!(%error, me, "game stats not saved");
    }
    let shells = reward(best);
    if shells == 0 {
        return Ok("Reach the 256 tile to earn shells.".to_owned());
    }
    let paid = pay_win(app, me, Game::Tide2048, shells, "tide2048_game").await?;
    Ok(if paid > 0 {
        format!("+**{paid}** shells!")
    } else {
        "(Today's reward limit for this game is reached.)".to_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_creature_has_a_sprite_and_a_name() {
        let sprites = pinchora_render::sprites::SpriteStore::embedded();
        for (id, name) in CREATURES {
            assert!(sprites.get(&format!("c_{id}")).is_some(), "{id} is not in the atlas");
            assert!(!name.is_empty());
        }
        assert_eq!(creature(1), "Shrimp");
        assert_eq!(creature(GOAL), "Leviathan");
        assert_eq!(creature(30), "Sunken chest", "bigger tiles keep the last creature");
    }

    #[test]
    fn a_new_board_has_two_tiles() {
        let board = new_board();
        assert_eq!(board.empties(), 14);
        assert!(board.best() <= 2);
    }
}
