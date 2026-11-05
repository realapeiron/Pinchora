use pinchora_db::RepOutcome;
use twilight_interactions::command::{CommandModel, CreateCommand, ResolvedUser};
use twilight_util::builder::embed::EmbedBuilder;

use crate::{
    Cx,
    util::{SUCCESS, mention, relative},
};

#[derive(CommandModel, CreateCommand)]
#[command(name = "rep", desc = "Give someone a reputation point (once a day)")]
pub struct Rep {
    /// Who deserves it
    user: ResolvedUser,
}

impl Rep {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let from = cx.author()?.id;
        let to = self.user.resolved.id;
        if to == from {
            return cx
                .reply_ephemeral("Nice try. Reputation has to come from someone else.")
                .await;
        }
        if self.user.resolved.bot {
            return cx.reply_ephemeral("Bots can't receive reputation.").await;
        }
        match cx.app.db.give_rep(from.get(), to.get()).await? {
            RepOutcome::Given { target_total } => {
                let embed = EmbedBuilder::new()
                    .color(SUCCESS)
                    .description(format!(
                        "{} gave {} a reputation point! They now have **{target_total}**.",
                        mention(from),
                        mention(to)
                    ))
                    .build();
                cx.reply_embed(embed).await
            }
            RepOutcome::Cooldown { next_at } => {
                cx.reply_ephemeral(format!("You can give reputation again {}.", relative(next_at)))
                    .await
            }
        }
    }
}
