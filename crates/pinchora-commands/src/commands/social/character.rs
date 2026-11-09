//! `/character`: one private editor message with a large live preview. A category menu picks
//! what to change (look, the six colours, accessories, presets, outfits), ◀ ▶ step through its
//! choices with one click each, a second menu jumps straight to one, and Undo, Randomise, Save
//! and Cancel work on a draft that is only kept on Save. The draft lives in Redis, so any pod can
//! answer a click; its session number rides in every custom id, so an older editor message cannot
//! overwrite a newer one.
//!
//! Custom ids: `ca:<action>:<session>[:<arg>]`. `ca:hex` and `ca:name` open modals
//! (`ca:hexm`, `ca:namem`). Old `ch:` selects from before the editor was rebuilt say it expired.

mod actions;
use actions::CharacterEdit;
mod view;

use std::{str::Split, time::Duration};

use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use pinchora_db::CustomizeBuy;
use pinchora_game::{
    Content,
    character::{
        Access, Accessory, Character, OUTFIT_NAME_MAX, OUTFIT_SLOTS, Shelf, accessories_in, accessory, preset, presets,
    },
};
use pinchora_render::character::{ACCESSORY_TINTS, Channel, Paint, Slot, parse_hex, shadeable};
use rand::{RngExt, SeedableRng, rngs::StdRng};
use serde::{Deserialize, Serialize};
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::channel::message::{
    Component,
    component::{Label, TextInput, TextInputStyle},
};

use crate::App;
use crate::Cx;
use crate::infra::util::thousands_in;

/// How long an untouched draft is kept.
const DRAFT_TTL: Duration = Duration::from_secs(2 * 3600);
/// Steps Undo can go back.
const UNDO_DEPTH: usize = 20;
/// The text box of the colour modal.
const HEX_FIELD: &str = "hex";

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "character",
    desc = "Design your pixel character: looks, colours, accessories and outfits"
)]
pub struct CharacterCommand;

impl CharacterCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let user = cx.author()?.id.get();
        let saved = saved_code(&cx.app, user).await?;
        let code = saved
            .clone()
            .unwrap_or_else(|| Character::starter(Content::get()).encode());
        let draft = Draft {
            session: rand::random_range(1..u32::MAX),
            code,
            saved: saved.unwrap_or_default(),
            undo: Vec::new(),
            tab: Tab::Look,
            slot: Slot::Head,
        };
        store(&cx.app, user, &draft).await?;
        let note = "Pick a category, then step through it with ◀ ▶ or jump with the menu. \
                    Nothing changes until you press **Save**.";
        let data = crate::commands::social::character::view::editor(cx, &draft, note).await?;
        cx.reply(data).await
    }
}

/// What the editor shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Tab {
    Look,
    Paint(Channel),
    Gear,
    Presets,
    Outfits,
}

impl Tab {
    pub(crate) const ALL: [Tab; 10] = [
        Tab::Look,
        Tab::Paint(Channel::Skin),
        Tab::Paint(Channel::Hair),
        Tab::Paint(Channel::Eyes),
        Tab::Paint(Channel::Top),
        Tab::Paint(Channel::Bottom),
        Tab::Paint(Channel::Shoes),
        Tab::Gear,
        Tab::Presets,
        Tab::Outfits,
    ];

    pub(crate) fn key(self) -> &'static str {
        match self {
            Tab::Look => "look",
            Tab::Paint(c) => c.key(),
            Tab::Gear => "gear",
            Tab::Presets => "presets",
            Tab::Outfits => "outfits",
        }
    }

    /// The tab's name in `lang`, from `character.tab.*` and `character.channel.*`.
    pub(crate) fn name(self, lang: Lang) -> String {
        match self {
            Tab::Paint(c) => channel_name(c, lang),
            other => t!(lang, &format!("character.tab.{}", other.key())),
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.key() == key)
    }
}

/// The character being edited, kept in Redis between clicks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Draft {
    pub(crate) session: u32,
    /// The character code as edited so far.
    pub(crate) code: String,
    /// What is saved; empty while the player never saved a character.
    pub(crate) saved: String,
    /// Earlier codes, the latest last.
    pub(crate) undo: Vec<String>,
    pub(crate) tab: Tab,
    /// The accessory slot open on the Accessories tab.
    pub(crate) slot: Slot,
}

impl Draft {
    pub(crate) fn character(&self) -> Character {
        let content = Content::get();
        Character::decode(content, &self.code).unwrap_or_else(|| Character::starter(content))
    }

    /// Makes `next` the draft, remembering the old one for Undo. False when nothing changed.
    fn change(&mut self, next: &Character) -> bool {
        let code = next.encode();
        if code == self.code {
            return false;
        }
        self.undo.push(std::mem::replace(&mut self.code, code));
        if self.undo.len() > UNDO_DEPTH {
            self.undo.remove(0);
        }
        true
    }

    pub(crate) fn is_saved(&self) -> bool {
        self.code == self.saved
    }
}

fn key(user: u64) -> String {
    format!("chardraft:{user}")
}

async fn store(app: &App, user: u64, draft: &Draft) -> anyhow::Result<()> {
    app.cache
        .put(&key(user), &serde_json::to_string(draft)?, DRAFT_TTL)
        .await
}

async fn load(app: &App, user: u64) -> anyhow::Result<Option<Draft>> {
    Ok(app
        .cache
        .get(&key(user))
        .await?
        .and_then(|json| serde_json::from_str(&json).ok()))
}

/// The saved code, if the player has a character that still reads.
async fn saved_code(app: &App, user: u64) -> anyhow::Result<Option<String>> {
    let content = Content::get();
    Ok(app
        .db
        .character(user)
        .await?
        .and_then(|code| Character::decode(content, &code))
        .map(|c| c.encode()))
}

/// What the player may wear, read from the database.
pub(crate) async fn load_shelf(app: &App, user: u64) -> anyhow::Result<Shelf> {
    Ok(Shelf::new(Content::get(), &app.db.wardrobe_facts(user).await?))
}

/// Words for what stands between a player and an accessory.
pub(crate) fn access_note(access: &Access, lang: Lang) -> String {
    match access {
        Access::Owned => t!(lang, "character.access.owned"),
        Access::Buy(price) => t!(lang, "character.access.buy", price = thousands_in(lang, *price)),
        Access::Level(at) => t!(lang, "character.access.level", level = at),
        Access::Badge(id) => t!(
            lang,
            "character.access.badge",
            badge = Content::get().badge(id).map_or(id.as_str(), |b| b.name.as_str())
        ),
    }
}

/// Answers a click on an editor that is no longer the live one.
async fn expired(cx: &Cx) -> anyhow::Result<()> {
    cx.reply_ephemeral(t!(cx.lang(), "character.expired")).await
}

/// `ch:` selects of the editor before character 2.0.
pub(crate) async fn on_old(cx: &Cx) -> anyhow::Result<()> {
    expired(cx).await
}

/// The step through a list: `forward` moves one on, else one back, wrapping round.
fn step<T: PartialEq + Clone>(list: &[T], current: Option<&T>, forward: bool) -> Option<T> {
    if list.is_empty() {
        return None;
    }
    let n = list.len();
    let at = current.and_then(|c| list.iter().position(|x| x == c));
    let next = match (at, forward) {
        (Some(i), true) => (i + 1) % n,
        (Some(i), false) => (i + n - 1) % n,
        (None, true) => 0,
        (None, false) => n - 1,
    };
    Some(list[next].clone())
}

/// The choices ◀ ▶ step through on the Accessories tab: nothing, then every item of the slot.
fn slot_choices(slot: Slot) -> Vec<Option<&'static str>> {
    std::iter::once(None)
        .chain(accessories_in(slot).map(|a| Some(a.id.as_str())))
        .collect()
}

/// `ca:<action>:<session>[:<arg>]` buttons and selects of the editor.
pub(crate) async fn on_component(cx: &Cx, mut args: Split<'_, char>, values: &[String]) -> anyhow::Result<()> {
    let user = cx.author()?.id.get();
    let action = args.next().unwrap_or_default();
    let session: Option<u32> = args.next().and_then(|s| s.parse().ok());
    let Some(draft) = load(&cx.app, user).await?.filter(|d| Some(d.session) == session) else {
        return expired(cx).await;
    };
    let value = values.first().map(String::as_str);
    let character = draft.character();
    let mut edit = CharacterEdit {
        cx,
        user,
        draft,
        character,
        note: String::new(),
    };
    let continued = match action {
        "tab" => edit.select_tab(value).await?,
        "slot" => edit.select_slot(&mut args).await?,
        "prev" | "next" => edit.step_choice(action).await?,
        "pick" => edit.pick_choice(value).await?,
        "undo" => edit.undo_change().await?,
        "rand" => edit.randomise_character().await?,
        "tint" => edit.tint_accessory().await?,
        "buy" => edit.buy_accessory().await?,
        "save" => edit.save_character().await?,
        "cancel" => edit.cancel_editor().await?,
        "keep" => edit.keep_outfit(&mut args).await?,
        "hex" => edit.choose_hex().await?,
        "name" => edit.name_outfits().await?,
        _ => return expired(cx).await,
    };
    if !continued {
        return Ok(());
    }
    let CharacterEdit {
        mut draft,
        character,
        mut note,
        ..
    } = edit;
    if draft.change(&character) && note.is_empty() {
        note = describe(&draft, &character, cx.lang());
    }
    store(&cx.app, user, &draft).await?;
    let data = crate::commands::social::character::view::editor(cx, &draft, &note).await?;
    cx.update(data).await
}

/// A one-line note on what the open tab now shows.
fn describe(draft: &Draft, character: &Character, lang: Lang) -> String {
    let content = Content::get();
    match draft.tab {
        Tab::Look => t!(
            lang,
            "character.now",
            what = Tab::Look.name(lang),
            value = content
                .look(&character.look)
                .map_or(character.look.as_str(), |l| l.name.as_str())
        ),
        Tab::Paint(channel) => t!(
            lang,
            "character.now",
            what = channel_name(channel, lang),
            value = channel.name_of(character.colors.get(channel))
        ),
        Tab::Gear => match character.worn(draft.slot).and_then(|w| accessory(&w.id)) {
            Some(item) => t!(lang, "character.now", what = draft.slot.label(), value = item.name),
            None => t!(lang, "character.now-nothing", what = draft.slot.label()),
        },
        Tab::Presets | Tab::Outfits => String::new(),
    }
}

/// [`Channel::label`] in `lang`.
pub(crate) fn channel_name(channel: Channel, lang: Lang) -> String {
    t!(lang, &format!("character.channel.{}", channel.key()))
}

/// A random character from what the player may wear: any look and swatch, and in each slot
/// either nothing or something they own.
pub fn randomise(content: &Content, shelf: &Shelf, rng: &mut impl RngExt) -> Character {
    let mut c = Character::starter(content);
    if !content.looks.is_empty() {
        c.look = content.looks[rng.random_range(0..content.looks.len())].id.clone();
    }
    for channel in Channel::ALL {
        let n = channel.swatches().len();
        c.colors.set(channel, Paint::Swatch(rng.random_range(0..n) as u8));
    }
    for slot in Slot::ALL {
        let owned: Vec<&Accessory> = accessories_in(slot)
            .filter(|a| shelf.access(a) == Access::Owned)
            .collect();
        if owned.is_empty() || rng.random_bool(0.45) {
            continue;
        }
        let item = owned[rng.random_range(0..owned.len())];
        let tint = (item.tintable && rng.random_bool(0.6))
            .then(|| Paint::Swatch(rng.random_range(0..ACCESSORY_TINTS.len()) as u8));
        c.wear(slot, Some(&item.id), tint);
    }
    c
}

/// A labelled one-line text box for a modal.
#[allow(deprecated)] // TextInput::label: the Label around it carries the label now
fn text_field(id: &str, label: &str, placeholder: &str, max: u16, value: Option<String>) -> Component {
    Component::Label(Label {
        component: Box::new(Component::TextInput(TextInput {
            custom_id: id.to_owned(),
            id: None,
            label: None,
            max_length: Some(max),
            min_length: Some(1),
            placeholder: Some(placeholder.to_owned()),
            required: Some(true),
            style: TextInputStyle::Short,
            value,
        })),
        description: None,
        id: None,
        label: label.to_owned(),
    })
}

/// `ca:hexm:<session>` and `ca:namem:<session>`: the colour code and outfit names typed in.
pub(crate) async fn on_modal(cx: &Cx, mut args: Split<'_, char>, fields: &[(String, String)]) -> anyhow::Result<()> {
    let user = cx.author()?.id.get();
    let action = args.next().unwrap_or_default();
    let session: Option<u32> = args.next().and_then(|s| s.parse().ok());
    let Some(mut draft) = load(&cx.app, user).await?.filter(|d| Some(d.session) == session) else {
        return cx.reply_ephemeral(t!(cx.lang(), "character.form-expired")).await;
    };
    let field = |id: &str| fields.iter().find(|(k, _)| k == id).map(|(_, v)| v.trim());
    let note;
    match action {
        "hexm" => {
            let Tab::Paint(channel) = draft.tab else {
                return cx.reply_ephemeral(t!(cx.lang(), "character.form-gone")).await;
            };
            match field(HEX_FIELD).and_then(parse_hex) {
                Some(typed) => {
                    let kept = shadeable(typed);
                    let mut character = draft.character();
                    character.colors.set(channel, Paint::Custom(kept));
                    draft.change(&character);
                    note = if kept == typed {
                        t!(
                            cx.lang(),
                            "character.now",
                            what = channel_name(channel, cx.lang()),
                            value = format!("#{typed:06x}")
                        )
                    } else {
                        t!(
                            cx.lang(),
                            "character.nudged",
                            what = channel_name(channel, cx.lang()),
                            kept = format!("#{kept:06x}"),
                            typed = format!("#{typed:06x}")
                        )
                    };
                }
                None => note = t!(cx.lang(), "character.not-hex"),
            }
        }
        "namem" => {
            let mut renamed = 0;
            for slot in 1..=OUTFIT_SLOTS {
                if let Some(name) = field(&format!("slot{slot}")).filter(|n| !n.is_empty()) {
                    let name: String = name.chars().take(OUTFIT_NAME_MAX).collect();
                    if cx.app.db.rename_outfit(user, slot, &name).await? {
                        renamed += 1;
                    }
                }
            }
            draft.tab = Tab::Outfits;
            note = t!(cx.lang(), "character.named", count = renamed);
        }
        _ => return cx.reply_ephemeral(t!(cx.lang(), "character.form-gone")).await,
    }
    store(&cx.app, user, &draft).await?;
    let data = crate::commands::social::character::view::editor(cx, &draft, &note).await?;
    cx.update(data).await
}

#[cfg(test)]
mod tests {
    use crate::commands::social::character::*;

    #[test]
    fn stepping_wraps_both_ways() {
        let list = [1, 2, 3];
        assert_eq!(step(&list, Some(&3), true), Some(1));
        assert_eq!(step(&list, Some(&1), false), Some(3));
        assert_eq!(
            step(&list, Some(&9), true),
            Some(1),
            "an unknown current starts at the front"
        );
        assert_eq!(step(&list, None, false), Some(3));
        assert_eq!(step::<i32>(&[], None, true), None);
    }

    #[test]
    fn tabs_round_trip_and_fit_one_menu() {
        for tab in Tab::ALL {
            assert_eq!(Tab::from_key(tab.key()), Some(tab));
            let json = serde_json::to_string(&tab).expect("serialises");
            assert_eq!(serde_json::from_str::<Tab>(&json).expect("parses"), tab);
        }
        assert!(Tab::ALL.len() <= 25);
    }

    #[test]
    fn randomise_only_dresses_in_what_is_owned() {
        let content = Content::get();
        let shelf = Shelf {
            bought: vec!["top_hat".to_owned()],
            level: 1,
            badges: Vec::new(),
        };
        let mut rng = StdRng::seed_from_u64(7);
        let mut seen_hat = false;
        for _ in 0..200 {
            let c = randomise(content, &shelf, &mut rng);
            assert!(Character::decode(content, &c.encode()).is_some());
            for w in &c.accessories {
                let item = accessory(&w.id).expect("real");
                assert_eq!(shelf.access(item), Access::Owned, "{} is not owned", w.id);
            }
            seen_hat |= c.worn(Slot::Head).is_some_and(|w| w.id == "top_hat");
        }
        assert!(seen_hat, "bought things come up too");
    }

    #[test]
    fn drafts_undo_and_survive_redis() {
        let content = Content::get();
        let mut draft = Draft {
            session: 5,
            code: Character::starter(content).encode(),
            saved: String::new(),
            undo: Vec::new(),
            tab: Tab::Paint(Channel::Hair),
            slot: Slot::Hand,
        };
        let mut c = draft.character();
        assert!(!draft.change(&c), "no change, no undo step");
        for i in 0..30u8 {
            c.colors.hair = Paint::Swatch(i % 20);
            draft.change(&c);
        }
        assert!(draft.undo.len() <= UNDO_DEPTH);
        let json = serde_json::to_string(&draft).expect("serialises");
        assert_eq!(serde_json::from_str::<Draft>(&json).expect("parses"), draft);
        assert!(json.len() < 4_000);
    }
}
