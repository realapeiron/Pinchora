use crate::admin::workflows::*;
fn text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max
}
impl Definition {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !text(&self.name, 40)
            || self
                .name
                .chars()
                .any(|c| !(c.is_alphanumeric() || c == '-' || c == '_'))
        {
            return Err("name");
        }
        if self.conditions.len() > MAX_CONDITIONS {
            return Err("conditions");
        }
        match &self.trigger {
            Trigger::RoleAdded { role_id } | Trigger::RoleRemoved { role_id } if !id(role_id) => return Err("trigger"),
            Trigger::ReactionAdded { emoji, channel_id } if !id(channel_id) || !valid_emoji(emoji) => {
                return Err("trigger");
            }
            Trigger::MessageKeyword { keyword } if !text(keyword, 200) => return Err("trigger"),
            Trigger::Schedule { spec, tz } if spec.next(DateTime::from_timestamp(0, 0).unwrap(), tz).is_none() => {
                return Err("trigger");
            }
            _ => {}
        }
        for c in &self.conditions {
            let ok = match c {
                Condition::HasRole { role_id } | Condition::LacksRole { role_id } => id(role_id),
                Condition::InChannel { channel_id } => id(channel_id),
                Condition::MinLevel { level } => *level <= 100000,
                Condition::AccountAge { days } => *days <= 36500,
                Condition::Chance { percent } => *percent <= 100,
                Condition::Cooldown { seconds, .. } => (1..=86400).contains(seconds),
                Condition::VarEquals { key, value, .. } => valid_key(key) && value.chars().count() <= 200,
                Condition::VarGreater { key, .. } => valid_key(key),
                Condition::TimeWindow { start, end, tz } => {
                    *start < 1440 && *end < 1440 && tz.parse::<chrono_tz::Tz>().is_ok()
                }
            };
            if !ok {
                return Err("conditions");
            }
        }
        let mut count = 0;
        if self.actions.is_empty() || !check(&self.actions, false, &mut count) {
            return Err("actions");
        }
        if self.actions.iter().map(StepAction::writes).sum::<usize>() > MAX_WRITES {
            return Err("writes");
        }
        Ok(())
    }
}

fn check(actions: &[StepAction], nested: bool, count: &mut usize) -> bool {
    for a in actions {
        *count += 1;
        let ok = match a {
            StepAction::Tag { action } => valid_tag(action),
            StepAction::TakePoints { amount } => (1..=1000).contains(amount),
            StepAction::SendMessage {
                channel_id,
                text: body,
                embed,
            } => id(channel_id) && text(body, 2000) && tags::clean_embed(embed.clone()).is_ok(),
            StepAction::Reply { text: body, embed } => text(body, 2000) && tags::clean_embed(embed.clone()).is_ok(),
            StepAction::SetVar { key, value, .. } => valid_key(key) && value.chars().count() <= 200,
            StepAction::IncrVar { key, amount, .. } => valid_key(key) && amount.unsigned_abs() <= 1_000_000_000,
            StepAction::CreateThread { channel_id, name } => id(channel_id) && text(name, 100),
            StepAction::AddReaction { emoji } => valid_emoji(emoji),
            StepAction::OpenTicket { panel_id, topic } => *panel_id > 0 && *topic < 25,
            StepAction::Log { text: body } => text(body, 2000),
            StepAction::Pin {} => true,
            StepAction::RandomBranch { branches } => {
                !nested
                    && !branches.is_empty()
                    && branches.len() <= 5
                    && branches.iter().all(|b| !b.is_empty() && check(b, true, count))
            }
        };
        if !ok || *count > MAX_ACTIONS {
            return false;
        }
    }
    true
}

fn valid_tag(action: &Action) -> bool {
    if let Some(role) = action.role() {
        return id(&role.to_string());
    }
    if let Action::React { emoji } = action {
        return valid_emoji(emoji);
    }
    tags::clean_actions(vec![action.clone()], true, true).is_ok()
}
fn valid_emoji(emoji: &str) -> bool {
    let Ok(clean) = crate::admin::settings::check_points_emoji(emoji) else {
        return false;
    };
    if clean.starts_with('<') {
        return clean
            .strip_suffix('>')
            .and_then(|s| s.rsplit(':').next())
            .is_some_and(id);
    }
    true
}
