use crate::custom;
use pinchora_core::{
    tags::Action,
    workflows::{self, Event, StepAction},
};

pub(super) fn rendered(action: &StepAction, event: &Event) -> Option<custom::Rendered> {
    let (text, embed) = match action {
        StepAction::SendMessage { text, embed, .. } | StepAction::Reply { text, embed } => (text, embed.as_ref()),
        StepAction::Tag {
            action: Action::Dm { text },
        } => (text, None),
        _ => return None,
    };
    Some(custom::render_message(
        &custom::Spec {
            responses: std::slice::from_ref(text),
            embed,
            allow_mentions: false,
        },
        &workflows::variables(event),
        &mut |n| event.roll as usize % n.max(1),
    ))
}

pub(super) fn preview(action: &StepAction, event: &Event) -> serde_json::Value {
    let mut detail = workflows::preview_action(action, event);
    if let Some(rendered) = rendered(action, event) {
        if matches!(action, StepAction::Tag { .. }) {
            detail["action"]["text"] = rendered.content.into();
        } else {
            detail["text"] = rendered.content.into();
            detail["embed"] = serde_json::to_value(rendered.embed).unwrap_or_default();
        }
    }
    if let StepAction::CreateThread { name, .. } = action {
        detail["name"] = pinchora_core::tags::truncate(&workflows::render(name, event), 100).into();
    }
    detail
}
