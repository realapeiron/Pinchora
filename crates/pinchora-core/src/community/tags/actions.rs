use super::*;

// /Actions

/// Something a custom command or auto-responder does besides answering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    AddRole {
        #[serde(with = "ids")]
        role_id: u64,
    },
    RemoveRole {
        #[serde(with = "ids")]
        role_id: u64,
    },
    ToggleRole {
        #[serde(with = "ids")]
        role_id: u64,
    },
    /// Gives (positive) or takes (negative) server points. Never shells: those are a global currency.
    GivePoints { amount: i64 },
    /// A direct message to the member.
    Dm { text: String },
    /// Reacts to the message that triggered a keyword responder.
    React { emoji: String },
}

impl Action {
    pub const fn key(&self) -> &'static str {
        match self {
            Self::AddRole { .. } => "add_role",
            Self::RemoveRole { .. } => "remove_role",
            Self::ToggleRole { .. } => "toggle_role",
            Self::GivePoints { .. } => "give_points",
            Self::Dm { .. } => "dm",
            Self::React { .. } => "react",
        }
    }

    /// The role the action hands out or takes, when it is a role action.
    pub const fn role(&self) -> Option<u64> {
        match self {
            Self::AddRole { role_id } | Self::RemoveRole { role_id } | Self::ToggleRole { role_id } => Some(*role_id),
            _ => None,
        }
    }
}

/// The actions made tidy and checked. `keyword` says whether a message can be reacted to, `member`
/// whether there is still a member to act on.
pub fn clean_actions(actions: Vec<Action>, keyword: bool, member: bool) -> Result<Vec<Action>, Vec<FieldError>> {
    let mut errors = Vec::new();
    if actions.len() > MAX_ACTIONS {
        errors.push(FieldError::new(
            "actions",
            format!("A command does at most {MAX_ACTIONS} things besides answering."),
        ));
    }
    let mut clean = Vec::with_capacity(actions.len());
    for (n, action) in actions.into_iter().enumerate() {
        let number = n + 1;
        if !member {
            errors.push(FieldError::new(
                "actions",
                format!("Action {number}: the member is gone by then, so there is nothing to do to them."),
            ));
            continue;
        }
        match action {
            Action::AddRole { role_id } | Action::RemoveRole { role_id } | Action::ToggleRole { role_id }
                if role_id == 0 =>
            {
                errors.push(FieldError::new("actions", format!("Action {number} needs a role.")));
            }
            Action::GivePoints { amount } if amount == 0 || !POINTS_ACTION.contains(&amount) => {
                errors.push(FieldError::new(
                    "actions",
                    format!(
                        "Action {number}: points go from {} to {}, and not 0.",
                        POINTS_ACTION.start(),
                        POINTS_ACTION.end()
                    ),
                ));
            }
            Action::Dm { text } => {
                let text = text.trim().to_owned();
                if text.is_empty() || text.chars().count() > RESPONSE_MAX {
                    errors.push(FieldError::new(
                        "actions",
                        format!("Action {number}: a direct message is 1 to {RESPONSE_MAX} characters."),
                    ));
                } else {
                    clean.push(Action::Dm { text });
                }
            }
            Action::React { .. } if !keyword => {
                errors.push(FieldError::new(
                    "actions",
                    format!("Action {number}: only a keyword responder has a message to react to."),
                ));
            }
            Action::React { emoji } => match check_points_emoji(&emoji) {
                Ok(emoji) => clean.push(Action::React { emoji }),
                Err(_) => errors.push(FieldError::new(
                    "actions",
                    format!(
                        "Action {number}: react with one emoji, a Unicode one like 🎉 or a custom one like <:name:123456789012345678>."
                    ),
                )),
            },
            other => clean.push(other),
        }
    }
    if errors.is_empty() { Ok(clean) } else { Err(errors) }
}

// /Texts
