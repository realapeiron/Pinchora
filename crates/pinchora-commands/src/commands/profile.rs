use chrono::Utc;
use pinchora_core::{economy::DAILY_STREAK_WINDOW, leveling};
use pinchora_render::{ProfileCard, Theme};
use twilight_interactions::command::{CommandModel, CreateCommand, ResolvedUser};

use crate::{Cx, util::resolved_display_name};

#[derive(CommandModel, CreateCommand)]
#[command(name = "profile", desc = "Show a profile card")]
pub struct Profile {
    /// Whose profile to show (default: you)
    user: Option<ResolvedUser>,
}

impl Profile {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let (user, name) = match &self.user {
            Some(r) => (r.resolved.clone(), resolved_display_name(r)),
            None => (cx.author()?.clone(), cx.author_display_name()),
        };
        if user.bot {
            return cx.reply_ephemeral("Bots don't have profiles.").await;
        }
        let db = &cx.app.db;
        let row = db.user(user.id.get()).await?;
        let global_rank = db.global_xp_rank(row.xp).await?;
        let server_rank = match cx.guild_id() {
            Some(guild) => db.member_rank(guild, user.id.get()).await?.map(|m| m.rank),
            None => None,
        };
        // A streak the user can no longer extend shows as 0.
        let streak = if row.last_daily_at.is_some_and(|t| Utc::now() - t <= DAILY_STREAK_WINDOW) {
            row.daily_streak
        } else {
            0
        };
        let avatar = cx.app.avatar(&user, ProfileCard::AVATAR_SIZE, Some(52)).await;
        let progress = leveling::progress(row.xp);
        let username = user.name.clone();
        let png = cx
            .app
            .render(move |r| {
                r.profile_card(&ProfileCard {
                    display_name: &name,
                    username: &username,
                    avatar_png: &avatar,
                    bio: &row.bio,
                    level: progress.level,
                    xp_current: progress.current,
                    xp_needed: progress.needed,
                    total_xp: row.xp,
                    credits: row.credits,
                    reputation: row.reputation,
                    streak,
                    global_rank,
                    server_rank,
                    theme: Theme::reef().with_accent(row.accent_color.map(|c| c as u32)),
                })
            })
            .await?;
        cx.reply_png("profile.png", png).await
    }
}
