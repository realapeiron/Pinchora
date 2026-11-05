use std::str::Split;

use pinchora_game::{
    Content,
    character::{Character, Field},
};
use pinchora_render::{
    CharacterCard, Theme,
    character::{HAIR_COLORS, OUTFIT_COLORS, SKIN_TONES, outfit_name},
};
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    channel::message::{
        Component,
        component::{ActionRow, SelectMenu, SelectMenuOption, SelectMenuType},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::EmbedBuilder;

use crate::{App, Cx, game::scene::scaled_png, util::BRAND};

#[derive(CommandModel, CreateCommand)]
#[command(name = "character", desc = "Design your pixel character")]
pub struct CharacterCommand;

impl CharacterCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let character = current(&cx.app, cx.author()?.id.get()).await?;
        let note = "Pick a look and colours; every change is saved right away. \
                    Your character stands in your house and joins you on visits.";
        let data = editor(cx, &character, note).await?;
        cx.reply(data).await
    }
}

/// The saved character, or the starter one while the player has not designed theirs.
async fn current(app: &App, user: u64) -> anyhow::Result<Character> {
    let content = Content::get();
    Ok(app
        .db
        .character(user)
        .await?
        .and_then(|code| Character::decode(content, &code))
        .unwrap_or_else(|| Character::starter(content)))
}

/// `ch:<field>` selects: saves the change, then redraws the editor.
pub(crate) async fn on_select(cx: &Cx, mut args: Split<'_, char>, value: Option<&str>) -> anyhow::Result<()> {
    let user = cx.author()?.id.get();
    let content = Content::get();
    let before = current(&cx.app, user).await?;
    let changed = args
        .next()
        .and_then(Field::from_key)
        .zip(value)
        .and_then(|(field, value)| before.with(content, field, value));
    let Some(after) = changed else {
        return cx
            .reply_ephemeral("That choice isn't available any more. Run `/character` again.")
            .await;
    };
    cx.app.db.set_character(user, &after.encode()).await?;
    let data = editor(cx, &after, "Saved.").await?;
    cx.update(data).await
}

fn option(label: String, value: String, current: bool) -> SelectMenuOption {
    SelectMenuOption {
        default: current,
        description: None,
        emoji: None,
        label,
        value,
    }
}

fn menu(field: Field, placeholder: &str, options: Vec<SelectMenuOption>) -> Component {
    Component::ActionRow(ActionRow {
        id: None,
        components: vec![Component::SelectMenu(SelectMenu {
            channel_types: None,
            custom_id: format!("ch:{}", field.key()),
            default_values: None,
            disabled: false,
            id: None,
            kind: SelectMenuType::Text,
            max_values: Some(1),
            min_values: Some(1),
            options: Some(options),
            placeholder: Some(placeholder.to_owned()),
            required: None,
        })],
    })
}

/// The ephemeral editor: the card and one select menu per field, each showing the current choice.
async fn editor(cx: &Cx, character: &Character, note: &str) -> anyhow::Result<InteractionResponseData> {
    let content = Content::get();
    let c = character.colors;
    let look = content
        .look(&character.look)
        .map_or_else(|| character.look.clone(), |l| l.name.clone());
    let png = {
        let (sprites, character, name) = (cx.app.sprites.clone(), character.clone(), cx.author_display_name());
        cx.app
            .render(move |r| {
                let (sprite_png, sprite_w, sprite_h) =
                    scaled_png(&sprites.character(&character), CharacterCard::SCALE)?;
                r.character_card(&CharacterCard {
                    sprite_png,
                    sprite_w,
                    sprite_h,
                    name,
                    look,
                    colors: character.colors,
                    theme: Theme::reef(),
                })
            })
            .await?
    };

    let looks = content
        .looks
        .iter()
        .map(|l| option(l.name.clone(), l.id.clone(), l.id == character.look))
        .collect();
    let named = |list: &[(&str, [u32; 4])], current: usize| {
        list.iter()
            .enumerate()
            .map(|(i, (name, _))| option((*name).to_owned(), i.to_string(), i == current))
            .collect()
    };
    let outfits = |current: usize| {
        (0..OUTFIT_COLORS.len())
            .map(|i| option(outfit_name(i), i.to_string(), i == current))
            .collect()
    };
    let components = vec![
        menu(Field::Look, "Look", looks),
        menu(Field::Skin, "Skin tone", named(&SKIN_TONES, c.skin)),
        menu(Field::Hair, "Hair colour", named(&HAIR_COLORS, c.hair)),
        menu(Field::Top, "Top colour", outfits(c.top)),
        menu(Field::Bottom, "Bottom colour", outfits(c.bottom)),
    ];
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title("Your character")
        .description(note);
    Ok(Cx::card(embed, "character.png", png, components, true))
}
