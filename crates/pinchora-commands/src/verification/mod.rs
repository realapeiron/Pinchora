//! F10 join gate, captcha interactions and timeout worker.
pub mod raid;
mod resume;
mod safety;
use crate::{
    App, Cx,
    hooks::JoinVerdict,
    server_events::{button, row},
};
use chrono::Duration as Span;
use pinchora_core::{
    i18n::Lang,
    modules::Module,
    t,
    verification::{self},
};
use pinchora_db::RaidJoin;
use pinchora_render::CaptchaCard;
use rand::RngExt;
pub use safety::check_roles;
use sha2::{Digest, Sha256};
use std::{str::Split, sync::Arc, time::Duration};
use twilight_model::{
    channel::message::{
        AllowedMentions, Component,
        component::{ButtonStyle, Label, TextInput, TextInputStyle},
    },
    gateway::payload::incoming::{MemberAdd, MemberRemove},
    id::Id,
};
use twilight_util::builder::embed::EmbedBuilder;

pub(crate) async fn on_member_join(app: &Arc<App>, event: &MemberAdd) -> JoinVerdict {
    match join(app, event).await {
        Ok(v) => v,
        Err(error) => {
            tracing::warn!(guild=event.guild_id.get(),%error,"join gate failed; holding join side effects");
            JoinVerdict::Held
        }
    }
}
async fn join(app: &Arc<App>, event: &MemberAdd) -> anyhow::Result<JoinVerdict> {
    let guild = event.guild_id.get();
    let user = event.user.id.get();
    if event.user.bot {
        return Ok(JoinVerdict::Continue);
    }
    let c = app.db.safety_config(guild).await?;
    let gate = c.verification.enabled && app.module_allowed(guild, None, Module::Verification).await;
    let raid_on = c.raid.enabled && app.module_allowed(guild, None, Module::AntiRaid).await;
    if !gate && !raid_on && app.db.active_raid(guild).await?.is_none() {
        return Ok(JoinVerdict::Continue);
    }
    if safety::member(app, guild, user, &c).await?.is_none() {
        return Ok(JoinVerdict::Continue);
    }
    let joined = event
        .joined_at
        .and_then(|t| chrono::DateTime::from_timestamp(t.as_secs(), 0))
        .unwrap_or(app.now());
    let _guard = lock(app, &format!("member:{guild}:{user}")).await?;
    if _guard.is_none() {
        return Ok(JoinVerdict::Held);
    }
    if let Some(p) = app.db.pending_verification(guild, user).await?
        && p.joined_at >= joined
        && p.verified_at.is_some()
    {
        return Ok(JoinVerdict::Continue);
    }
    let active = raid::joined(
        app,
        guild,
        &RaidJoin {
            guild_id: guild as i64,
            user_id: user as i64,
            joined_at: joined,
            name_family: pinchora_core::anti_raid::name_family(&event.user.name),
            avatar: event.user.avatar.map(|a| a.to_string()),
            young: verification::age_hours(user, app.now()) < i64::from(c.raid.new_account_hours),
            status: "joined".into(),
        },
        &c,
    )
    .await?;
    if let Some(e) = &active
        && matches!(e.action.as_str(), "kick" | "ban")
    {
        safety::punish(app, guild, user, &c, e.action == "ban", None).await?;
        return Ok(JoinVerdict::Held);
    }
    let forced = active.is_some_and(|e| e.action != "alert");
    if !gate && !forced {
        return Ok(JoinVerdict::Continue);
    }
    let flagged = verification::flagged(
        &c.verification,
        user,
        &event.user.name,
        event.user.avatar.is_none(),
        app.now(),
    );
    let lang = app.guild_lang(guild).await;
    if flagged && c.verification.rule_action == "kick" {
        safety::punish(app, guild, user, &c, false, None).await?;
        return Ok(JoinVerdict::Held);
    }
    if flagged {
        let text = t!(lang, "verification.flagged", user = user);
        let _ = safety::log(
            app,
            guild,
            app.application_id.get(),
            Some(user),
            "verification.flagged",
            &text,
            vec![],
        )
        .await;
        if c.verification.dm_on_block {
            dm(app, user, &text).await;
        }
    }
    let expires =
        (c.verification.kick_after_mins > 0).then(|| joined + Span::minutes(i64::from(c.verification.kick_after_mins)));
    if app
        .db
        .pending_verification(guild, user)
        .await?
        .is_none_or(|p| p.joined_at < joined)
    {
        app.cache.captcha_clear(guild, user).await?;
    }
    app.db.hold_verification(guild, user, joined, expires).await?;
    app.db.raid_join_status(guild, user, "held").await?;
    if let Some(role) = &c.verification.unverified_role_id {
        safety::role(app, guild, user, role, true).await?;
    }
    if c.verification.dm_on_block {
        dm(app, user, &t!(lang, "verification.prompt")).await;
    }
    Ok(JoinVerdict::Held)
}
async fn dm(app: &App, user: u64, text: &str) {
    if let Some(channel) = crate::leveling::private_channel(app, user).await {
        let _ = app
            .http
            .create_message(channel)
            .content(text)
            .allowed_mentions(Some(&AllowedMentions::default()))
            .await;
    }
}
pub(crate) async fn on_member_leave(app: &Arc<App>, event: &MemberRemove) {
    let _ = app.cache.captcha_clear(event.guild_id.get(), event.user.id.get()).await;
    if let Err(error) = app
        .db
        .forget_verification(event.guild_id.get(), event.user.id.get())
        .await
    {
        tracing::warn!(%error,"could not clear verification for leaver");
    }
}
fn nonce() -> String {
    format!("{:032x}", rand::random::<u128>())
}
fn digest(nonce: &str, answer: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest(format!("{nonce}:{}", answer.trim().to_ascii_uppercase()).as_bytes())
    )
}
pub async fn challenge(app: &Arc<App>, guild: u64, user: u64, lang: Lang) -> anyhow::Result<(String, Vec<u8>)> {
    let pending = app.db.pending_verification(guild, user).await?;
    anyhow::ensure!(
        pending.is_some_and(|p| p.verified_at.is_none()),
        t!(lang, "verification.expired")
    );
    let nonce = nonce();
    let answer: String = {
        let mut rng = rand::rng();
        let alphabet = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
        (0..5)
            .map(|_| alphabet[rng.random_range(0..alphabet.len())] as char)
            .collect()
    };
    anyhow::ensure!(
        app.cache
            .captcha_issue(guild, user, &nonce, &digest(&nonce, &answer))
            .await?,
        t!(lang, "verification.cooldown")
    );
    let seed = rand::random::<u64>();
    let png = app
        .render(move |r| {
            r.captcha_card(&CaptchaCard {
                answer: &answer,
                seed,
                lang,
            })
        })
        .await?;
    Ok((nonce, png))
}
pub async fn answer(
    app: &Arc<App>,
    guild: u64,
    user: u64,
    nonce: &str,
    text: &str,
    lang: Lang,
) -> anyhow::Result<String> {
    anyhow::ensure!(
        nonce.len() == 32 && nonce.bytes().all(|c| c.is_ascii_hexdigit()) && text.len() <= 32,
        t!(lang, "verification.expired")
    );
    let Some(_guard) = lock(app, &format!("member:{guild}:{user}")).await? else {
        return Ok(t!(lang, "verification.busy"));
    };
    match app
        .cache
        .captcha_answer(guild, user, nonce, &digest(nonce, text))
        .await?
    {
        1 => {
            release(app, guild, user).await?;
            Ok(t!(lang, "verification.success"))
        }
        0 => {
            app.db.verification_attempt(guild, user).await?;
            Ok(t!(lang, "verification.wrong"))
        }
        -2 => {
            app.db.verification_attempt(guild, user).await?;
            Ok(t!(lang, "verification.cooldown"))
        }
        _ => Ok(t!(lang, "verification.expired")),
    }
}
async fn release(app: &Arc<App>, guild: u64, user: u64) -> anyhow::Result<()> {
    let lang = app.guild_lang(guild).await;
    let p = app
        .db
        .pending_verification(guild, user)
        .await?
        .filter(|p| p.verified_at.is_none())
        .ok_or_else(|| anyhow::anyhow!(t!(lang, "verification.expired")))?;
    let c = app.db.safety_config(guild).await?;
    let Some(m) = safety::member(app, guild, user, &c).await? else {
        app.db.finish_verification(guild, user, app.now()).await?;
        return Ok(());
    };
    // Never grant a role for a challenge issued before a leave/rejoin.
    anyhow::ensure!(
        m.joined_at.is_some_and(|t| t.as_secs() <= p.joined_at.timestamp()),
        t!(lang, "verification.expired")
    );
    if let Some(id) = &c.verification.verified_role_id {
        safety::role(app, guild, user, id, true).await?;
    }
    if let Some(id) = &c.verification.unverified_role_id {
        safety::role(app, guild, user, id, false).await?;
    }
    if app.db.finish_verification(guild, user, app.now()).await? {
        let _ = safety::log(
            app,
            guild,
            app.application_id.get(),
            Some(user),
            "verification.passed",
            &t!(lang, "verification.passed", user = user),
            vec![],
        )
        .await;
        if let Err(error) = resume::member(app, guild, m).await {
            tracing::warn!(guild,user,%error,"verified member welcome failed");
        }
    }
    Ok(())
}
pub(crate) async fn begin(cx: &Cx) -> anyhow::Result<()> {
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral(t!(cx.lang(), "verification.guild_only")).await;
    };
    let user = cx.author()?.id.get();
    let c = cx.app.db.safety_config(guild).await?;
    let active = cx.app.db.active_raid(guild).await?.is_some_and(|r| r.action != "alert");
    if !c.verification.enabled && !active {
        return cx.reply_ephemeral(t!(cx.lang(), "verification.disabled")).await;
    }
    let Some(p) = cx.app.db.pending_verification(guild, user).await? else {
        return cx.reply_ephemeral(t!(cx.lang(), "verification.no_pending")).await;
    };
    if p.verified_at.is_some() {
        return cx.reply_ephemeral(t!(cx.lang(), "verification.success")).await;
    }
    if c.verification.mode == "button" && !active {
        let Some(_guard) = lock(&cx.app, &format!("member:{guild}:{user}")).await? else {
            return cx.reply_ephemeral(t!(cx.lang(), "verification.busy")).await;
        };
        release(&cx.app, guild, user).await?;
        return cx.reply_ephemeral(t!(cx.lang(), "verification.success")).await;
    }
    if c.verification.mode == "web" && !active {
        let token = nonce();
        cx.app
            .cache
            .put(
                &format!("vf:web:{token}"),
                &format!("{guild}:{user}"),
                Duration::from_secs(600),
            )
            .await?;
        return cx
            .reply_ephemeral(t!(
                cx.lang(),
                "verification.web_link",
                url = format!("{}/verify/{token}", cx.app.public_url.trim_end_matches('/'))
            ))
            .await;
    }
    let (nonce, png) = match challenge(&cx.app, guild, user, cx.lang()).await {
        Ok(c) => c,
        Err(e) => return cx.reply_ephemeral(e.to_string()).await,
    };
    let controls = vec![row(vec![
        button(
            format!("cq:a:{nonce}"),
            &t!(cx.lang(), "verification.enter"),
            None,
            ButtonStyle::Primary,
            false,
        ),
        button(
            "vf:help".into(),
            &t!(cx.lang(), "verification.ask"),
            None,
            ButtonStyle::Secondary,
            false,
        ),
        button(
            "vf:go".into(),
            &t!(cx.lang(), "verification.retry"),
            None,
            ButtonStyle::Secondary,
            false,
        ),
    ])];
    cx.reply(Cx::card(
        EmbedBuilder::new().description(t!(cx.lang(), "verification.instructions")),
        "captcha.png",
        png,
        controls,
        true,
    ))
    .await
}
pub(crate) async fn on_component(cx: &Cx, prefix: &str, mut parts: Split<'_, char>) -> anyhow::Result<()> {
    let action = parts.next().unwrap_or_default();
    if prefix == "vf" {
        return match action {
            "go" => begin(cx).await,
            "help" => {
                let guild = cx
                    .guild_id()
                    .ok_or_else(|| anyhow::anyhow!(t!(cx.lang(), "verification.guild_only")))?;
                let user = cx.author()?.id.get();
                if cx
                    .app
                    .db
                    .pending_verification(guild, user)
                    .await?
                    .is_none_or(|p| p.verified_at.is_some())
                {
                    return cx.reply_ephemeral(t!(cx.lang(), "verification.no_pending")).await;
                }
                if cx
                    .app
                    .cache
                    .once(&format!("vf:help:{guild}:{user}"), Duration::from_secs(300))
                    .await?
                {
                    safety::log(
                        &cx.app,
                        guild,
                        user,
                        Some(user),
                        "verification.help",
                        &t!(cx.lang(), "verification.help_alert", user = user),
                        vec![],
                    )
                    .await?;
                }
                cx.reply_ephemeral(t!(cx.lang(), "verification.help_sent")).await
            }
            _ => cx.reply_ephemeral(t!(cx.lang(), "verification.expired")).await,
        };
    }
    if prefix == "cq" && action == "a" {
        let Some(nonce) = parts
            .next()
            .filter(|n| n.len() == 32 && n.bytes().all(|c| c.is_ascii_hexdigit()))
        else {
            return cx.reply_ephemeral(t!(cx.lang(), "verification.expired")).await;
        };
        return cx
            .modal(
                format!("cq:a:{nonce}"),
                &t!(cx.lang(), "verification.modal_title"),
                vec![field(cx.lang())],
            )
            .await;
    }
    if prefix == "rd" {
        if !cx.can_manage_guild() {
            return cx.reply_ephemeral(t!(cx.lang(), "verification.manage")).await;
        }
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "verification.guild_only")).await;
        };
        let Some(id) = parts.next().and_then(|p| p.parse::<i64>().ok()) else {
            return cx.reply_ephemeral(t!(cx.lang(), "verification.expired")).await;
        };
        let actor = cx.author()?.id.get();
        match action {
            "unlock" => raid::unlock(&cx.app, guild, actor, Some(id)).await?,
            "kick" | "ban" => raid::act_window(&cx.app, guild, id, action == "ban", Some(actor)).await?,
            _ => return cx.reply_ephemeral(t!(cx.lang(), "verification.expired")).await,
        }
        return cx.reply_ephemeral(t!(cx.lang(), "verification.done")).await;
    }
    cx.reply_ephemeral(t!(cx.lang(), "verification.expired")).await
}
#[allow(deprecated)]
fn field(lang: Lang) -> Component {
    Component::Label(Label {
        id: None,
        label: t!(lang, "verification.answer"),
        description: None,
        component: Box::new(Component::TextInput(TextInput {
            custom_id: "answer".into(),
            id: None,
            label: None,
            max_length: Some(5),
            min_length: Some(5),
            placeholder: None,
            required: Some(true),
            style: TextInputStyle::Short,
            value: None,
        })),
    })
}
pub(crate) async fn on_modal(cx: &Cx, mut parts: Split<'_, char>, fields: &[(String, String)]) -> anyhow::Result<()> {
    let action = parts.next();
    let nonce = parts.next().unwrap_or_default();
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral(t!(cx.lang(), "verification.guild_only")).await;
    };
    if action != Some("a") {
        return cx.reply_ephemeral(t!(cx.lang(), "verification.expired")).await;
    }
    let value = fields
        .iter()
        .find(|(k, _)| k == "answer")
        .map_or("", |(_, v)| v.as_str());
    let result = answer(&cx.app, guild, cx.author()?.id.get(), nonce, value, cx.lang()).await?;
    cx.reply_ephemeral(result).await
}
pub async fn web_identity(app: &App, token: &str, user: u64) -> anyhow::Result<u64> {
    anyhow::ensure!(
        token.len() == 32 && token.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid token"
    );
    let raw = app
        .cache
        .get(&format!("vf:web:{token}"))
        .await?
        .ok_or_else(|| anyhow::anyhow!("expired token"))?;
    let (g, u) = raw.split_once(':').ok_or_else(|| anyhow::anyhow!("invalid token"))?;
    anyhow::ensure!(u == user.to_string(), "token belongs to another member");
    Ok(g.parse()?)
}
pub async fn post(app: &Arc<App>, guild: u64) -> anyhow::Result<()> {
    let c = app.db.safety_config(guild).await?;
    let lang = app.guild_lang(guild).await;
    let channel = c
        .verification
        .channel_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!(t!(lang, "verification.channel_required")))?
        .parse()?;
    let found = app.http.channel(Id::new(channel)).await?.model().await?;
    anyhow::ensure!(
        found.guild_id.map(|g| g.get()) == Some(guild),
        t!(lang, "verification.invalid")
    );
    let text = if c.verification.message.is_empty() {
        t!(lang, "verification.prompt")
    } else {
        crate::util::strip_mentions(&c.verification.message)
    };
    let components = vec![row(vec![
        button(
            "vf:go".into(),
            &t!(lang, "verification.verify"),
            None,
            ButtonStyle::Primary,
            false,
        ),
        button(
            "vf:help".into(),
            &t!(lang, "verification.ask"),
            None,
            ButtonStyle::Secondary,
            false,
        ),
    ])];
    if let Some(id) = c
        .verification
        .message_id
        .as_deref()
        .and_then(|id| id.parse::<u64>().ok())
    {
        match app
            .http
            .update_message(Id::new(channel), Id::new(id))
            .content(Some(&text))
            .components(Some(&components))
            .allowed_mentions(Some(&AllowedMentions::default()))
            .await
        {
            Ok(_) => return Ok(()),
            Err(error) if crate::roles::error_code(&error) == Some(10008) => {}
            Err(error) => return Err(error.into()),
        }
    }
    let message = app
        .http
        .create_message(Id::new(channel))
        .content(&text)
        .components(&components)
        .allowed_mentions(Some(&AllowedMentions::default()))
        .await?
        .model()
        .await?;
    app.db.verification_posted(guild, message.id.get()).await
}
pub async fn tick(app: &Arc<App>) -> anyhow::Result<()> {
    for p in app.db.due_verifications(app.now()).await? {
        let (g, u) = (p.guild_id as u64, p.user_id as u64);
        let Some(_guard) = lock(app, &format!("member:{g}:{u}")).await? else {
            continue;
        };
        if app
            .db
            .pending_verification(g, u)
            .await?
            .is_none_or(|p| p.verified_at.is_some() || p.expires_at.is_none_or(|e| e > app.now()))
        {
            continue;
        }
        let c = app.db.safety_config(g).await?;
        if !c.verification.enabled
            || c.verification.kick_after_mins == 0
            || !app.module_allowed(g, None, Module::Verification).await
        {
            continue;
        }
        if let Err(error) = safety::punish(app, g, u, &c, false, None).await {
            tracing::warn!(g,u,%error,"verification timeout action failed");
        }
    }
    for e in app.db.due_raids(app.now()).await? {
        if let Err(error) = raid::unlock(app, e.guild_id as u64, app.application_id.get(), Some(e.id)).await {
            tracing::warn!(raid=e.id,%error,"raid restoration failed; will retry");
        }
    }
    app.db.sweep_safety(app.now() - Span::days(30)).await?;
    Ok(())
}
pub fn spawn_ticker(app: Arc<App>) {
    let tasks = app.clone();
    app.spawn(async move {
        let mut timer = tokio::time::interval(Duration::from_secs(30));
        loop {
            timer.tick().await;
            if let Err(error) = tick(&tasks).await {
                tracing::warn!(%error,"safety ticker failed");
            }
        }
    });
}

/// A live raid must always leave its captcha and emergency exit accessible.
pub(crate) async fn active_gate(cx: &Cx, module: Module) -> anyhow::Result<bool> {
    let Some(guild) = cx.guild_id() else { return Ok(false) };
    Ok(cx
        .app
        .db
        .active_raid(guild)
        .await?
        .is_some_and(|e| module == Module::AntiRaid || e.action != "alert"))
}

/// A throttled help request, available from Discord and the member's web challenge.
pub async fn request_help(app: &Arc<App>, guild: u64, user: u64, lang: Lang) -> anyhow::Result<String> {
    anyhow::ensure!(
        app.db
            .pending_verification(guild, user)
            .await?
            .is_some_and(|p| p.verified_at.is_none()),
        t!(lang, "verification.no_pending")
    );
    if app
        .cache
        .once(&format!("vf:help:{guild}:{user}"), Duration::from_secs(300))
        .await?
    {
        safety::log(
            app,
            guild,
            user,
            Some(user),
            "verification.help",
            &t!(lang, "verification.help_alert", user = user),
            vec![],
        )
        .await?;
    }
    Ok(t!(lang, "verification.help_sent"))
}

async fn lock(app: &App, key: &str) -> anyhow::Result<Option<pinchora_cache::VerificationLease>> {
    let owner = nonce();
    // Drop releases asynchronously; allow that release to reach Redis before reporting contention.
    for _ in 0..3 {
        if let Some(lease) = app.cache.verification_lease(key, &owner).await? {
            return Ok(Some(lease));
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Ok(None)
}
