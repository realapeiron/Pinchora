use super::*;

// /Components

pub(super) fn button_if(custom_id: String, label: &str, style: ButtonStyle, disabled: bool) -> Component {
    Component::Button(Button {
        custom_id: Some(custom_id),
        disabled,
        emoji: None,
        id: None,
        label: Some(label.to_owned()),
        sku_id: None,
        style,
        url: None,
    })
}

pub(super) fn button(custom_id: String, label: &str, style: ButtonStyle) -> Component {
    button_if(custom_id, label, style, false)
}

pub(super) fn row(components: Vec<Component>) -> Component {
    Component::ActionRow(ActionRow { id: None, components })
}

pub(super) fn select(custom_id: String, placeholder: &str, options: Vec<SelectMenuOption>) -> Component {
    row(vec![Component::SelectMenu(SelectMenu {
        channel_types: None,
        custom_id,
        default_values: None,
        disabled: false,
        id: None,
        kind: SelectMenuType::Text,
        max_values: Some(1),
        min_values: Some(1),
        options: Some(options),
        placeholder: Some(placeholder.to_owned()),
        required: None,
    })])
}
