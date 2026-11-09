use super::*;

pub(super) struct LogChanges {
    pub(super) events: Vec<(LogEvent, bool)>,
    pub(super) categories: Vec<(LogCategory, Option<u64>)>,
    pub(super) default_channel: Option<Option<u64>>,
    pub(super) lists: [Option<Vec<u64>>; 3],
}

pub(super) fn parse_log_changes(body: &LogsBody) -> ApiResult<LogChanges> {
    let mut errors = Vec::new();
    let mut note = |result: Result<(), FieldError>| {
        if let Err(error) = result {
            errors.push(error);
        }
    };
    let events: Vec<(LogEvent, bool)> = body
        .events
        .iter()
        .flatten()
        .filter_map(|(key, on)| match LogEvent::parse(key) {
            Some(event) => Some((event, *on)),
            None => {
                note(Err(FieldError::new(
                    "events",
                    format!("There is no event called {key}."),
                )));
                None
            }
        })
        .collect();
    let mut categories: Vec<(LogCategory, Option<u64>)> = Vec::new();
    for (key, raw) in body.category_channels.iter().flatten() {
        let Some(category) = LogCategory::parse(key) else {
            note(Err(FieldError::new(
                "category_channels",
                format!("There is no category called {key}."),
            )));
            continue;
        };
        match raw.as_deref().map(parse_id) {
            Some(Some(id)) => categories.push((category, Some(id))),
            Some(None) => note(Err(FieldError::new("category_channels", "Channel ids are numbers."))),
            None => categories.push((category, None)),
        }
    }
    let default_channel = match &body.channel_id {
        Some(Some(raw)) => match parse_id(raw) {
            Some(id) => Some(Some(id)),
            None => {
                note(Err(FieldError::new("channel_id", "Channel ids are numbers.")));
                None
            }
        },
        Some(None) => Some(None),
        None => None,
    };
    let mut lists = [None, None, None];
    for (n, (field, raw)) in [
        ("ignored_channels", &body.ignored_channels),
        ("ignored_roles", &body.ignored_roles),
        ("ignored_users", &body.ignored_users),
    ]
    .into_iter()
    .enumerate()
    {
        if let Some(raw) = raw {
            match id_list(field, raw) {
                Ok(list) => lists[n] = Some(list),
                Err(error) => note(Err(error)),
            }
        }
    }
    if !errors.is_empty() {
        return Err(ApiError::invalid(errors));
    }

    Ok(LogChanges {
        events,
        categories,
        default_channel,
        lists,
    })
}

pub(super) async fn check_log_destinations(
    state: &WebState,
    guild: u64,
    now: &LogConfig,
    default_channel: Option<Option<u64>>,
    categories: &[(LogCategory, Option<u64>)],
    lists: &[Option<Vec<u64>>; 3],
) -> ApiResult<()> {
    // Channels and roles new to the settings must exist; ones kept may have gone since.
    let new_posts: Vec<(&'static str, u64)> = default_channel
        .flatten()
        .filter(|id| Some(*id) != now.default_channel)
        .map(|id| ("channel_id", id))
        .into_iter()
        .chain(
            categories
                .iter()
                .filter_map(|(c, id)| id.filter(|id| now.channels.get(c) != Some(id)))
                .map(|id| ("category_channels", id)),
        )
        .collect();
    let new_channels: Vec<u64> = lists[0]
        .iter()
        .flatten()
        .copied()
        .filter(|id| !now.ignored_channels.contains(id))
        .collect();
    let new_roles: Vec<u64> = lists[1]
        .iter()
        .flatten()
        .copied()
        .filter(|id| !now.ignored_roles.contains(id))
        .collect();
    if !new_posts.is_empty() || !new_channels.is_empty() || !new_roles.is_empty() {
        let info = guild_info::get_with(state, guild, |info| {
            new_posts.iter().all(|(_, id)| info.channel(*id).is_some())
                && new_channels.iter().all(|id| info.channel(*id).is_some())
                && new_roles.iter().all(|id| info.role(*id).is_some())
        })
        .await?;
        let mut errors = Vec::new();
        for (field, id) in &new_posts {
            if let Err(error) = info.check_post_channel(field, *id) {
                errors.push(error);
            }
        }
        for id in &new_channels {
            if let Err(error) = info.check_channel("ignored_channels", *id) {
                errors.push(error);
            }
        }
        for id in &new_roles {
            if let Err(error) = info.check_role("ignored_roles", *id) {
                errors.push(error);
            }
        }
        if !errors.is_empty() {
            return Err(ApiError::invalid(errors));
        }
    }

    Ok(())
}
