//! The case reversal used by commands and the dashboard.

use pinchora_core::infra::i18n::Lang;
use pinchora_core::moderation::moderation::{CaseKind, may_act, outranks};
use pinchora_core::moderation::moderation::Rank;
use pinchora_core::t;
use pinchora_db::{CaseRow, NewCase, OpenedCase};
use sqlx::{Postgres, Transaction};
use twilight_http::request::AuditLogReason;
use twilight_model::guild::Permissions;
use twilight_model::id::Id;
use std::{collections::HashMap, sync::Arc, time::Duration};

use super::{Act, Moderator, Ranks, post_case};
use crate::App;
use crate::commands::moderation::members::audit_reason;

// Bounds the pooled connection and member lock while leaving room for a Discord rate-limit wait.
const DISCORD_REVERSAL_WAIT: Duration = Duration::from_secs(8);

impl Ranks {
    async fn read_fresh(app: &App, guild: u64, moderator: &Moderator) -> Option<Self> {
        let (facts, roles, mine) = tokio::join!(
            app.guild_facts(guild),
            app.http.roles(Id::new(guild.max(1))),
            app.my_roles(guild),
        );
        let facts = facts?;
        let roles = roles.ok()?.models().await.ok()?;
        let mine = mine.ok()?;
        let table: Arc<HashMap<u64, super::RoleInfo>> = Arc::new(roles.into_iter().map(|r| (r.id.get(), super::RoleInfo {
            position: r.position, permissions: r.permissions,
        })).collect());
        Some(Self {
            owner: facts.owner,
            actor: Rank { id: moderator.id, top: super::top(&table, moderator.roles.iter().copied()) },
            bot: Rank { id: app.application_id.get(), top: super::top(&table, mine.iter().copied()) },
            table,
        })
    }
}

pub struct RevokeActor<'a> {
    pub id: u64,
    pub name: &'a str,
    pub roles: Vec<u64>,
    pub source: &'a str,
    pub target_name: Option<&'a str>,
    pub language: Lang,
}

#[derive(Debug)]
pub enum RevokeError {
    Missing,
    Inactive,
    NotTimedOut(String),
    Kind,
    Hierarchy(String),
    Permission(String),
    Discord(String),
    Internal(anyhow::Error),
}

impl From<anyhow::Error> for RevokeError {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(error)
    }
}
impl From<sqlx::Error> for RevokeError {
    fn from(error: sqlx::Error) -> Self {
        Self::Internal(error.into())
    }
}

/// Reverse one active member case. The case lock makes a second request see the first one's result.
pub async fn revoke_case(
    app: &App,
    guild: u64,
    number: i32,
    actor: RevokeActor<'_>,
    reason: Option<&str>,
) -> Result<OpenedCase, RevokeError> {
    let server = app.guild_lang(guild).await;
    let preview = app.db.case(guild, number).await?.ok_or(RevokeError::Missing)?;
    let mut tx = app.db.pool().begin().await.map_err(anyhow::Error::from)?;
    if matches!(preview.kind(), Some(CaseKind::Ban | CaseKind::Timeout)) {
        pinchora_db::Db::lock_reversal_member_in(&mut tx, guild, preview.target_id as u64, preview.kind().unwrap()).await?;
    }
    pinchora_db::Db::lock_case_in(&mut tx, guild, number).await?;
    let case = pinchora_db::Db::case_in(&mut tx, guild, number).await?.ok_or(RevokeError::Missing)?;
    let kind = match case.kind() {
        Some(CaseKind::Warn) if !case.kind().is_some_and(CaseKind::on_channel) => CaseKind::Unwarn,
        Some(CaseKind::Timeout) if !case.kind().is_some_and(CaseKind::on_channel) => CaseKind::Untimeout,
        Some(CaseKind::Ban) if !case.kind().is_some_and(CaseKind::on_channel) => CaseKind::Unban,
        _ => return Err(RevokeError::Kind),
    };
    if !case.active {
        return Err(RevokeError::Inactive);
    }
    // Role and member reads under the locks share the reversal's bound.
    tokio::time::timeout(DISCORD_REVERSAL_WAIT, check_revoke(app, guild, &case, kind, &actor))
        .await
        .map_err(|_| RevokeError::Discord(t!(actor.language, "mod.discord-no-response")))??;
    let target = case.target_id as u64;
    let moderator = Moderator {
        id: actor.id,
        name: actor.name.to_owned(),
        roles: actor.roles,
    };
    let lang = actor.language;
    if kind == CaseKind::Unwarn {
        let details = t!(server, "mod.details.unwarn", number = number);
        if !pinchora_db::Db::close_reversal_in(&mut tx, &case).await? {
            return Err(RevokeError::Inactive);
        }
        let opened = pinchora_db::Db::open_reversal_in(&mut tx, &NewCase {
            guild, kind, target, target_name: &case.target_name,
            moderator: moderator.id, moderator_name: &moderator.name, reason,
            duration: None, expires_at: None, details: Some(&details),
        }, actor.source).await?;
        tx.commit().await.map_err(anyhow::Error::from)?;
        post_case(app, guild, &opened.case).await;
        return Ok(opened);
    }
    let audit = audit_reason(lang, &moderator, reason);
    let done = tokio::time::timeout(DISCORD_REVERSAL_WAIT, async {
        if kind == CaseKind::Unban {
            app.http.delete_ban(Id::new(guild), Id::new(target)).reason(&audit).await.map(drop)
        } else {
            app.http.update_guild_member(Id::new(guild), Id::new(target))
                .communication_disabled_until(None).reason(&audit).await.map(drop)
        }
    }).await.map_err(|_| RevokeError::Discord(t!(lang, "mod.discord-no-response")))?;
    if let Err(error) = done
        && (kind != CaseKind::Unban || crate::roles::roles::error_code(&error) != Some(10026))
    {
        return Err(RevokeError::Discord(super::members::refusal(lang, &error)));
    }
    let act = Act {
        kind,
        target,
        target_name: actor.target_name.unwrap_or(&case.target_name).to_owned(),
        reason: reason.map(str::to_owned),
        duration: None,
        details: None,
    };
    let new = NewCase {
        guild, kind, target, target_name: &act.target_name,
        moderator: moderator.id, moderator_name: &moderator.name, reason,
        duration: None, expires_at: None, details: None,
    };
    let mut opened = None;
    for attempt in 0..2 {
        sqlx::query("SAVEPOINT reversal_write").execute(&mut *tx).await?;
        let write = async {
            if !pinchora_db::Db::close_reversal_in(&mut tx, &case).await? {
                return Ok(None);
            }
            pinchora_db::Db::open_reversal_in(&mut tx, &new, actor.source).await.map(Some)
        }.await;
        match write {
            Ok(Some(saved)) => { opened = Some(saved); break; }
            Ok(None) => return Err(RevokeError::Inactive),
            Err(error) => {
                sqlx::query("ROLLBACK TO SAVEPOINT reversal_write").execute(&mut *tx).await?;
                if attempt == 1 {
                    tracing::error!(guild, case = number, %error, "Discord reversal succeeded but case write failed twice");
                    return Err(RevokeError::Internal(error));
                }
            }
        }
    }
    let opened = opened.expect("one of two writes succeeded");
    tx.commit().await.map_err(|error| {
        tracing::error!(guild, case = number, %error, "Discord reversal succeeded but case commit failed");
        RevokeError::Internal(error.into())
    })?;
    super::case_record::finish_record(app, guild, &act, None, opened).await.map_err(Into::into)
}

pub(crate) async fn record_locked(
    app: &App, mut tx: Transaction<'_, Postgres>, guild: u64,
    moderator: (u64, &str), act: &Act, told: Option<bool>,
) -> anyhow::Result<OpenedCase> {
    let opened = pinchora_db::Db::open_reversal_in(&mut tx, &NewCase {
        guild, kind: act.kind, target: act.target, target_name: &act.target_name,
        moderator: moderator.0, moderator_name: moderator.1, reason: act.reason.as_deref(),
        duration: act.duration,
        expires_at: act.duration.filter(|_| act.kind == CaseKind::Ban).map(|d| app.now() + d),
        details: act.details.as_deref(),
    }, "command").await?;
    tx.commit().await?;
    super::case_record::finish_record(app, guild, act, told, opened).await
}

async fn check_revoke(
    app: &App,
    guild: u64,
    case: &CaseRow,
    kind: CaseKind,
    actor: &RevokeActor<'_>,
) -> Result<(), RevokeError> {
    let moderator = Moderator {
        id: actor.id,
        name: actor.name.to_owned(),
        roles: actor.roles.clone(),
    };
    let lang = actor.language;
    let ranks = if kind == CaseKind::Unban && actor.source == "command" {
        None
    } else {
        Some(
            if actor.source == "web" { Ranks::read_fresh(app, guild, &moderator).await } else { Ranks::read(app, guild, &moderator).await }
                .ok_or_else(|| RevokeError::Hierarchy(t!(lang, "mod.no-roles")))?,
        )
    };
    let target = case.target_id as u64;
    if kind == CaseKind::Unwarn {
        let ranks = ranks.as_ref().expect("warn ranks");
        if target == moderator.id {
            return Err(RevokeError::Hierarchy(t!(lang, "mod.own-warn")));
        }
        let member = ranks
            .of(app, guild, target)
            .await
            .ok_or_else(|| RevokeError::Hierarchy(t!(lang, "mod.no-roles")))?;
        if !outranks(ranks.owner, ranks.actor, member) {
            return Err(RevokeError::Hierarchy(t!(lang, "mod.unwarn-above")));
        }
        let giver = case.moderator_id as u64;
        if giver != moderator.id && giver != ranks.bot.id && giver != 0 {
            let rank = ranks
                .of(app, guild, giver)
                .await
                .ok_or_else(|| RevokeError::Hierarchy(t!(lang, "mod.no-roles")))?;
            if !outranks(ranks.owner, ranks.actor, rank) {
                return Err(RevokeError::Hierarchy(t!(
                    lang,
                    "mod.unwarn-moderator",
                    moderator = format!("<@{giver}>")
                )));
            }
        }
    }
    if kind == CaseKind::Untimeout {
        let ranks = ranks.as_ref().expect("timeout ranks");
        let member = ranks
            .of(app, guild, target)
            .await
            .ok_or_else(|| RevokeError::Hierarchy(t!(lang, "mod.no-roles")))?;
        if let Err(why) = may_act(ranks.owner, ranks.actor, ranks.bot, member) {
            return Err(RevokeError::Hierarchy(super::words::refusal(lang, why, "untimeout")));
        }
    }
    if actor.source == "web" {
        let ranks = ranks.as_ref().expect("dashboard ranks");
        let held = moderator.roles.iter().copied().chain([guild]);
        let powers = held
            .filter_map(|id| ranks.table.get(&id))
            .fold(Permissions::empty(), |all, role| all | role.permissions);
        let needed = if kind == CaseKind::Unban {
            Permissions::BAN_MEMBERS
        } else {
            Permissions::MODERATE_MEMBERS
        };
        if moderator.id != ranks.owner && !powers.contains(Permissions::ADMINISTRATOR) && !powers.contains(needed) {
            let key = if kind == CaseKind::Unban {
                "ban-members"
            } else {
                "timeout-members"
            };
            return Err(RevokeError::Permission(t!(
                lang,
                "mod.needs",
                permission = super::words::permission(lang, key)
            )));
        }
    }
    if kind == CaseKind::Untimeout && actor.source == "web" {
        let member = tokio::time::timeout(DISCORD_REVERSAL_WAIT, async {
            app.http.guild_member(Id::new(guild), Id::new(target)).await
                .map_err(|error| RevokeError::Discord(super::members::refusal(lang, &error)))?
                .model().await.map_err(|error| RevokeError::Internal(error.into()))
        }).await.map_err(|_| RevokeError::Discord(t!(lang, "mod.discord-no-response")))??;
        if member.communication_disabled_until.is_none_or(|until| until.as_secs() <= app.now().timestamp()) {
            return Err(RevokeError::NotTimedOut(t!(lang, "mod.not-timed-out")));
        }
    }
    Ok(())
}
