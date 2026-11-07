use chrono::Utc;
use pinchora_core::{economy::DAILY_STREAK_WINDOW, leveling, t};
use pinchora_db::UserRow;
use pinchora_game::{Background, Badge, Content, character::Character};
use pinchora_render::{ProfileCard, SpriteArt};
use twilight_interactions::command::{CommandModel, CreateCommand, ResolvedUser};
use twilight_model::user::User;

use crate::{
    Cx,
    commands::{
        customize::{self, Cosmetics, Showcase},
        partner::partner_name,
        privacy,
    },
    game::scene::scaled_png,
    util::resolved_display_name,
};

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
            return cx.reply_ephemeral(t!(cx.lang(), "error.bot-no-profile")).await;
        }
        if privacy::private_profile(cx, user.id.get()).await? {
            return Ok(());
        }
        let row = cx.app.db.user(user.id.get()).await?;
        let background = row.background.as_deref().and_then(|id| Content::get().background(id));
        let badges = customize::worn_badges(&cx.app, user.id.get(), &row).await?;
        let png = card(cx, &user, name, row, background, &badges).await?;
        cx.reply_png("profile.png", png).await
    }
}

/// Draws `user`'s profile card. The background and badges are passed in, rather than read from
/// `row`, so `/customize` can show a choice before it is made.
pub(crate) async fn card(
    cx: &Cx,
    user: &User,
    name: String,
    row: UserRow,
    background: Option<&Background>,
    badges: &[&Badge],
) -> anyhow::Result<Vec<u8>> {
    let showcase = Showcase::load(&cx.app, user.id.get(), &row, cx.lang()).await?;
    card_with(cx, user, name, row, background, badges, showcase).await
}

/// Like [`card`], with the showcase already loaded: an editor that has read it for its own sake
/// does not read it a second time.
pub(crate) async fn card_with(
    cx: &Cx,
    user: &User,
    name: String,
    row: UserRow,
    background: Option<&Background>,
    badges: &[&Badge],
    showcase: Showcase,
) -> anyhow::Result<Vec<u8>> {
    let db = &cx.app.db;
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
    let avatar = cx.app.avatar(user, ProfileCard::AVATAR_SIZE, Some(52)).await;
    let progress = leveling::progress(row.xp);
    let username = user.name.clone();
    let character = row
        .character
        .as_deref()
        .and_then(|code| Character::decode(Content::get(), code));
    let sprites = cx.app.sprites.clone();
    let cosmetics = Cosmetics::new(&sprites, background, badges);
    let theme = customize::theme(&row, background);
    let lang = cx.lang();
    let title = customize::title_name(&row, lang);
    let partner = partner_name(&cx.app, user.id.get()).await;
    let birthday_today = if let Some(guild) = cx.guild_id() {
        db.birthday_profile(guild, user.id.get())
            .await?
            .is_some_and(|(month, day, zone, public)| {
                let visible = public || cx.author().is_ok_and(|viewer| viewer.id == user.id);
                visible
                    && pinchora_core::birthdays::timezone(&zone).is_some_and(|tz| {
                        let today = pinchora_core::birthdays::local_date(cx.app.now(), tz);
                        pinchora_core::birthdays::birthday_this_year(
                            month as u32,
                            day as u32,
                            chrono::Datelike::year(&today),
                        ) == Some(today)
                    })
            })
    } else {
        false
    };
    cx.app
        .render(move |r| {
            let art = character
                .map(|c| scaled_png(&sprites.character(&c), ProfileCard::CHARACTER_SCALE))
                .transpose()?;
            let icons = cosmetics.icons();
            let showcase_art = showcase.art(&sprites);
            let slots = Showcase::slots(&showcase_art);
            r.profile_card(&ProfileCard {
                lang,
                display_name: &name,
                title: title.as_deref(),
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
                character: art.as_ref().map(|(png, w, h)| SpriteArt { png, w: *w, h: *h }),
                backdrop: cosmetics.backdrop(),
                badges: &icons,
                showcase: &slots,
                partner,
                birthday_today,
                theme,
            })
        })
        .await
}
