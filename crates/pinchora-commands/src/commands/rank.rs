use pinchora_core::leveling;
use pinchora_game::Content;
use pinchora_render::{NextRole, RankCard};
use twilight_interactions::command::{CommandModel, CreateCommand, ResolvedUser};

use crate::{
    Cx,
    commands::{
        customize::{Cosmetics, theme, title_name},
        privacy,
    },
    roles,
    util::resolved_display_name,
};

#[derive(CommandModel, CreateCommand)]
#[command(name = "rank", desc = "Show a server level card", contexts = "guild")]
pub struct Rank {
    /// Whose rank to show (default: you)
    user: Option<ResolvedUser>,
}

impl Rank {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral("Ranks only exist inside a server.").await;
        };
        let (user, name) = match &self.user {
            Some(r) => (r.resolved.clone(), resolved_display_name(r)),
            None => (cx.author()?.clone(), cx.author_display_name()),
        };
        if user.bot {
            return cx.reply_ephemeral("Bots don't earn XP.").await;
        }
        if privacy::private_profile(cx, user.id.get()).await? {
            return Ok(());
        }
        let db = &cx.app.db;
        let (xp, rank) = match db.member_rank(guild, user.id.get()).await? {
            Some(m) => (m.xp, m.rank),
            None => (0, db.guild_member_count(guild).await? + 1),
        };
        let progress = leveling::progress(xp);
        let next_role = roles::next_role(&cx.app, guild, progress.level).await?;
        let avatar = cx.app.avatar(&user, RankCard::AVATAR_SIZE, Some(50)).await;
        let row = db.user(user.id.get()).await?;
        let background = row.background.as_deref().and_then(|id| Content::get().background(id));
        let cosmetics = Cosmetics::new(&cx.app.sprites, background, &[]);
        let theme = theme(&row, background);
        let title = title_name(&row);
        let username = user.name.clone();
        let png = cx
            .app
            .render(move |r| {
                r.rank_card(&RankCard {
                    display_name: &name,
                    title,
                    username: &username,
                    avatar_png: &avatar,
                    level: progress.level,
                    xp_current: progress.current,
                    xp_needed: progress.needed,
                    rank,
                    next_role: next_role.as_ref().map(|(name, level)| NextRole { name, level: *level }),
                    backdrop: cosmetics.backdrop(),
                    theme,
                })
            })
            .await?;
        cx.reply_png("rank.png", png).await
    }
}
