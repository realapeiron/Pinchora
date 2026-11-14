use super::*;

// /Actions

/// How long a role change an auto-responder made is remembered, so the member update it causes does not
/// set role responders off again.
const OWN_CHANGE: Duration = Duration::from_secs(15);

/// What the actions of a command or responder need to know about who they act on.
pub(crate) struct Actor<'a> {
    pub guild: u64,
    pub user: u64,
    /// The roles the member holds, for a toggle.
    pub held: &'a [u64],
    /// The message that set off a keyword responder, to react to.
    pub message: Option<(Id<ChannelMarker>, Id<MessageMarker>)>,
    /// The language of the notes about actions that did not work.
    pub lang: Lang,
    /// An auto-responder runs them: its role changes must not set other responders off (a chain stops
    /// after one responder).
    pub responder: bool,
}

/// The cache key that marks a role change an auto-responder made.
pub(crate) fn own_change_key(guild: u64, user: u64, role: u64) -> String {
    format!("auto-own:{guild}:{user}:{role}")
}

fn utc_day() -> i64 {
    chrono::Utc::now().timestamp().div_euclid(86_400)
}

/// How many of `amount` points the member may still get from custom actions today, counted against
/// [`ACTION_POINTS_DAILY_MAX`]; `0` when the count cannot be kept.
async fn points_allowance(app: &App, guild: u64, user: u64, amount: i64) -> i64 {
    let key = format!("custom-points:{guild}:{user}:{}", utc_day());
    match app.cache.incr_by(&key, amount, Duration::from_secs(2 * 86_400)).await {
        Ok(total) => (ACTION_POINTS_DAILY_MAX - (total - amount)).clamp(0, amount),
        Err(error) => {
            tracing::warn!(guild, user, %error, "could not count custom action points");
            0
        }
    }
}

/// Whether one more direct message from a custom action or auto-responder may go to the member of
/// `guild` this hour (at most [`ACTION_DMS_PER_HOUR`]). A count that cannot be kept sends nothing.
pub(crate) async fn dm_allowed(app: &App, guild: u64, user: u64) -> bool {
    let hour = chrono::Utc::now().timestamp().div_euclid(3_600);
    let key = format!("custom-dm:{guild}:{user}:{hour}");
    match app.cache.incr(&key, Duration::from_secs(2 * 3_600)).await {
        Ok(sent) => sent <= ACTION_DMS_PER_HOUR,
        Err(error) => {
            tracing::warn!(guild, user, %error, "could not count custom direct messages");
            false
        }
    }
}

/// What the points actions did: the member's balance if one ran, and a word when today's cap held some
/// back.
#[derive(Debug, Default)]
pub(crate) struct PointsRun {
    pub balance: Option<i64>,
    pub capped: Option<String>,
}

/// Runs the points actions, which only touch the database. A server with points off skips them; what they
/// give one member a day is capped at [`ACTION_POINTS_DAILY_MAX`] (taking is not).
pub(crate) async fn run_points(app: &App, lang: Lang, guild: u64, user: u64, actions: &[Action]) -> PointsRun {
    let mut run = PointsRun::default();
    for action in actions {
        let Action::GivePoints { amount } = action else {
            continue;
        };
        let settings = match app.guild_settings(guild).await {
            Ok(settings) if settings.points_enabled => settings,
            _ => continue,
        };
        let done = if *amount > 0 {
            let allowed = points_allowance(app, guild, user, *amount).await;
            if allowed < *amount {
                run.capped = Some(t!(
                    lang,
                    "customcommand.capped",
                    max = thousands_in(lang, ACTION_POINTS_DAILY_MAX),
                    name = settings.points_name
                ));
            }
            if allowed == 0 {
                continue;
            }
            app.db.give_points(guild, user, allowed).await
        } else {
            app.db
                .take_points(guild, user, amount.abs())
                .await
                .map(|(_, balance)| balance)
        };
        match done {
            Ok(points) => run.balance = Some(points),
            Err(error) => tracing::warn!(guild, user, %error, "a points action failed"),
        }
    }
    run
}

/// The permission bits of the server's roles as Discord has them now, for the roles a run would give.
async fn live_role_permissions(app: &App, guild: u64) -> anyhow::Result<Vec<(u64, u64)>> {
    let roles = app.http.roles(Id::new(guild.max(1))).await?.models().await?;
    Ok(roles.into_iter().map(|r| (r.id.get(), r.permissions.bits())).collect())
}

/// Runs the actions that talk to Discord (roles, direct messages, reactions), after the answer went
/// out. Returns what to tell the member about the ones that did not work.
pub(crate) async fn run_discord_actions(app: &App, who: &Actor<'_>, actions: &[Action], vars: &Vars) -> Vec<String> {
    let mut notes = Vec::new();
    let (guild_id, user_id) = (Id::new(who.guild.max(1)), Id::new(who.user.max(1)));
    let gives = |action: &Action| match action {
        Action::AddRole { .. } => true,
        Action::ToggleRole { role_id } => !who.held.contains(role_id),
        _ => false,
    };
    // A role is checked again as it is now: its permissions may have changed since it was put here.
    let permissions = if actions.iter().any(gives) {
        live_role_permissions(app, who.guild)
            .await
            .map_err(|error| tracing::warn!(guild = who.guild, %error, "could not check the roles to give"))
            .ok()
    } else {
        None
    };
    for action in actions {
        match action {
            Action::AddRole { .. } | Action::RemoveRole { .. } | Action::ToggleRole { .. } => {
                let role = action.role().unwrap_or(0);
                let give = gives(action);
                if give {
                    let refusal = match &permissions {
                        None => Some(t!(who.lang, "customcommand.unchecked", role = format!("<@&{role}>"))),
                        Some(list) => list
                            .iter()
                            .find(|(id, _)| *id == role)
                            .and_then(|(_, bits)| run_time_refusal_in(who.lang, *bits, &format!("<@&{role}>"))),
                    };
                    if let Some(why) = refusal {
                        tracing::warn!(guild = who.guild, role, "a role action named a role it may not give");
                        notes.push(why);
                        continue;
                    }
                }
                if who.responder {
                    // Marked before the change: Discord may tell of the member update before it answers.
                    let key = own_change_key(who.guild, who.user, role);
                    if let Err(error) = app.cache.put(&key, "1", OWN_CHANGE).await {
                        tracing::warn!(guild = who.guild, %error, "could not mark a responder's role change");
                    }
                }
                let role_id = Id::new(role.max(1));
                let result = if give {
                    app.http
                        .add_guild_member_role(guild_id, user_id, role_id)
                        .reason("Custom command")
                        .await
                } else {
                    app.http
                        .remove_guild_member_role(guild_id, user_id, role_id)
                        .reason("Custom command")
                        .await
                };
                if let Err(error) = result {
                    tracing::warn!(guild = who.guild, role, %error, "a role action failed");
                    notes.push(t!(
                        who.lang,
                        if give { "roles.cant-give" } else { "roles.cant-take" },
                        role = format!("<@&{role}>"),
                        why = crate::roles::roles::refusal_in(who.lang, &error)
                    ));
                }
            }
            Action::Dm { text } => {
                if !dm_allowed(app, who.guild, who.user).await {
                    tracing::debug!(user = who.user, "a direct message action was over the hourly limit");
                    continue;
                }
                let clean = scrub_template(text, false);
                let text = truncate(render(&clean, vars, &mut pick).trim(), RESPONSE_MAX);
                if let Some(channel) = crate::progression::leveling::private_channel(app, who.user).await {
                    let none = AllowedMentions::default();
                    if let Err(error) = app
                        .http
                        .create_message(channel)
                        .content(&text)
                        .allowed_mentions(Some(&none))
                        .await
                    {
                        tracing::debug!(user = who.user, %error, "a direct message action was not delivered");
                    }
                }
            }
            Action::React { emoji } => {
                let Some((channel, message)) = who.message else {
                    continue;
                };
                react(app, channel, message, emoji).await;
            }
            Action::GivePoints { .. } => {}
        }
    }
    notes
}

/// Reacts to a message with a Unicode emoji or `<:name:id>`.
async fn react(app: &App, channel: Id<ChannelMarker>, message: Id<MessageMarker>, emoji: &str) {
    use twilight_http::request::channel::reaction::RequestReactionType;
    let custom = emoji
        .strip_prefix('<')
        .and_then(|e| e.strip_suffix('>'))
        .map(|e| e.strip_prefix('a').unwrap_or(e))
        .and_then(|e| e.strip_prefix(':'))
        .and_then(|e| e.split_once(':'))
        .and_then(|(name, id)| Some((name, id.parse::<u64>().ok()?)));
    let result = match custom {
        Some((name, id)) => {
            let kind = RequestReactionType::Custom {
                id: Id::new(id.max(1)),
                name: Some(name),
            };
            app.http.create_reaction(channel, message, &kind).await
        }
        None => {
            let kind = RequestReactionType::Unicode { name: emoji };
            app.http.create_reaction(channel, message, &kind).await
        }
    };
    if let Err(error) = result {
        tracing::debug!(%error, "a reaction action failed");
    }
}
