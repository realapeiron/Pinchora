//! `/party stop`: the host of the game in this channel, or a manager, ends it. Nothing is paid and nothing
//! is refunded: the party games have no wagers, and a game that was stopped counts for nobody.

use pinchora_core::t;
use pinchora_db::Commit;
use pinchora_game::party::Party;

use crate::Cx;
use crate::commands::games::games::channel_of;
use crate::commands::games::party_games::edit_card;
use crate::commands::games::party_games::game_lang;
use crate::commands::games::party_games::lobby;
use crate::infra::util::mention;

/// Ends the lobby or game running in this channel, and closes its message.
pub(crate) async fn run(cx: &Cx) -> anyhow::Result<()> {
    let app = &cx.app;
    let user = cx.author()?.id.get();
    let lang = cx.lang();
    // A click or the clock may move the game between reading it and ending it: look again.
    for _ in 0..3 {
        let Some(game) = app.db.party_in_channel(channel_of(cx), app.now()).await? else {
            return cx.reply_ephemeral(t!(lang, "party.stop.none")).await;
        };
        if user != game.host() && !cx.can_manage_guild() {
            return cx
                .reply_ephemeral(t!(lang, "party.stop.denied", host = mention(game.host())))
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
        // What the table reads is in the game's language, what the one who stopped it reads is in theirs.
        let table = game_lang(&done.game);
        let named =
            |lang| Party::from_key(&game.kind).map_or_else(|| t!(lang, "party.the-party-game"), |p| p.label_in(lang));
        let shown = edit_card(
            app,
            &done.game,
            &lobby::closed(t!(
                table,
                "party.stop.stopped",
                user = mention(user),
                label = named(table)
            )),
        )
        .await;
        let key = if shown {
            "party.stop.done"
        } else {
            "party.stop.done-unseen"
        };
        return cx.reply_ephemeral(t!(lang, key, label = named(lang))).await;
    }
    cx.reply_ephemeral(t!(lang, "party.stop.kept-changing")).await
}
