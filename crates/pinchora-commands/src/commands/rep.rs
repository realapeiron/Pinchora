use pinchora_core::t;
use pinchora_db::RepOutcome;
use twilight_interactions::command::{CommandModel, CreateCommand, ResolvedUser};
use twilight_util::builder::embed::EmbedBuilder;

use crate::{
    Cx, roles,
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
            return cx.reply_ephemeral(t!(cx.lang(), "rep.self")).await;
        }
        if self.user.resolved.bot {
            return cx.reply_ephemeral(t!(cx.lang(), "rep.bot")).await;
        }
        match cx.app.db.give_rep(from.get(), to.get()).await? {
            RepOutcome::Given { target_total } => {
                let mut text = t!(
                    cx.lang(),
                    "rep.given",
                    from = mention(from),
                    to = mention(to),
                    total = target_total
                );
                // Reputation roles are the server's own reward for a member it can see.
                if let (Some(guild), Some(member)) = (cx.guild_id(), self.user.member.as_ref()) {
                    let held: Vec<u64> = member.roles.iter().map(|r| r.get()).collect();
                    for role in roles::give_rep_roles(&cx.app, guild, to.get(), target_total, &held).await {
                        text.push('\n');
                        text.push_str(&t!(
                            cx.lang(),
                            "rep.earned",
                            user = mention(to),
                            role = format!("<@&{role}>")
                        ));
                    }
                }
                let embed = EmbedBuilder::new().color(SUCCESS).description(text).build();
                cx.reply_embed(embed).await
            }
            RepOutcome::Cooldown { next_at } => {
                cx.reply_ephemeral(t!(cx.lang(), "rep.cooldown", when = relative(next_at)))
                    .await
            }
        }
    }
}
