//! `/party stop`: the host of the game in this channel, or a manager, ends it. Nothing is paid and nothing
//! is refunded: the party games have no wagers, and a game that was stopped counts for nobody.

use pinchora_db::Commit;
use pinchora_game::party::Party;

use super::{edit_card, lobby};
use crate::{Cx, commands::games::channel_of, util::mention};

/// Ends the lobby or game running in this channel, and closes its message.
pub(crate) async fn run(cx: &Cx) -> anyhow::Result<()> {
    let app = &cx.app;
    let user = cx.author()?.id.get();
    // A click or the clock may move the game between reading it and ending it: look again.
    for _ in 0..3 {
        let Some(game) = app.db.party_in_channel(channel_of(cx), app.now()).await? else {
            return cx
                .reply_ephemeral("There is no party game to stop in this channel.")
                .await;
        };
        if user != game.host() && !cx.can_manage_guild() {
            return cx
                .reply_ephemeral(format!(
                    "Only {} or a member who can manage the server can stop this game.",
                    mention(game.host())
                ))
                .await;
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
        let Some(done) = app.db.commit_party(game.id, game.seq, commit).await? else {
            continue;
        };
        let label = Party::from_key(&game.kind).map_or("The party game", Party::label);
        let shown = edit_card(
            app,
            &done.game,
            &lobby::closed(format!(
                "🛑 {} stopped {label}. Nobody won, and no shells were paid out.",
                mention(user)
            )),
        )
        .await;
        return cx
            .reply_ephemeral(if shown {
                format!("Stopped {label}.")
            } else {
                format!("Stopped {label}, but its message could not be updated: it may have been deleted.")
            })
            .await;
    }
    cx.reply_ephemeral("The game kept changing. Try stopping it again in a moment!")
        .await
}
