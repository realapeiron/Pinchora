use pinchora_core::{
    leveling::{self, LevelReward},
    t,
};
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
    /// Show the separate voice level instead of chat level
    voice: Option<bool>,
}

impl Rank {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "rank.guild-only")).await;
        };
        let (user, name) = match &self.user {
            Some(r) => (r.resolved.clone(), resolved_display_name(r)),
            None => (cx.author()?.clone(), cx.author_display_name()),
        };
        if user.bot {
            return cx.reply_ephemeral(t!(cx.lang(), "error.bot-no-xp")).await;
        }
        if privacy::private_profile(cx, user.id.get()).await? {
            return Ok(());
        }
        let db = &cx.app.db;
        let voice = self.voice.unwrap_or(false);
        let (xp, rank) = if voice {
            db.voice_level_rank(guild, user.id.get()).await?
        } else {
            match db.member_rank(guild, user.id.get()).await? {
                Some(m) => (m.xp, m.rank),
                None => (0, db.guild_member_count(guild).await? + 1),
            }
        };
        let progress = db.leveling_parity(guild).await?.progress(xp);
        let next_role = if voice {
            let rewards: Vec<LevelReward> = db
                .voice_level_roles(guild)
                .await?
                .into_iter()
                .map(|r| LevelReward {
                    level: r.level as u32,
                    role_id: r.role_id as u64,
                })
                .collect();
            leveling::next_reward(&rewards, progress.level)
                .map(|reward| (format!("<@&{}>", reward.role_id), reward.level))
        } else {
            roles::next_role(&cx.app, guild, progress.level).await?
        };
        let avatar = cx.app.avatar(&user, RankCard::AVATAR_SIZE, Some(50)).await;
        let row = db.user(user.id.get()).await?;
        let background = row.background.as_deref().and_then(|id| Content::get().background(id));
        let cosmetics = Cosmetics::new(&cx.app.sprites, background, &[]);
        let theme = theme(&row, background);
        let lang = cx.lang();
        let title = title_name(&row, lang);
        let username = user.name.clone();
        let png = cx
            .app
            .render(move |r| {
                r.rank_card(&RankCard {
                    lang,
                    display_name: &name,
                    title: title.as_deref(),
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
        cx.reply_png(if voice { "voice-rank.png" } else { "rank.png" }, png)
            .await
    }
}
