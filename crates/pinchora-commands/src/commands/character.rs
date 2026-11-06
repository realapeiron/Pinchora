//! `/character`: one private editor message with a large live preview. A category menu picks
//! what to change (look, the six colours, accessories, presets, outfits), ◀ ▶ step through its
//! choices with one click each, a second menu jumps straight to one, and Undo, Randomise, Save
//! and Cancel work on a draft that is only kept on Save. The draft lives in Redis, so any pod can
//! answer a click; its session number rides in every custom id, so an older editor message cannot
//! overwrite a newer one.
//!
//! Custom ids: `ca:<action>:<session>[:<arg>]`. `ca:hex` and `ca:name` open modals
//! (`ca:hexm`, `ca:namem`). Old `ch:` selects from before the editor was rebuilt say it expired.

mod view;

use std::{str::Split, time::Duration};

use pinchora_core::leveling;
use pinchora_db::CustomizeBuy;
use pinchora_game::{
    Content,
    character::{
        Access, Accessory, Character, OUTFIT_NAME_MAX, OUTFIT_SLOTS, accessories_in, accessory, preset, presets,
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

use crate::{App, Cx, commands::customize::earned_badges, util::thousands};

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
        let data = view::editor(cx, &draft, note).await?;
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

    pub(crate) fn label(self) -> &'static str {
        match self {
            Tab::Look => "Look",
            Tab::Paint(c) => c.label(),
            Tab::Gear => "Accessories",
            Tab::Presets => "Presets",
            Tab::Outfits => "Outfits",
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

/// What the player may wear: what they bought, their level and their badges.
pub(crate) struct Shelf {
    pub(crate) bought: Vec<String>,
    pub(crate) level: u32,
    pub(crate) badges: Vec<String>,
}

impl Shelf {
    pub(crate) async fn load(app: &App, user: u64) -> anyhow::Result<Self> {
        let row = app.db.user(user).await?;
        Ok(Self {
            bought: app.db.owned_accessories(user).await?,
            level: leveling::level_of(row.xp),
            badges: earned_badges(app, user).await?,
        })
    }

    pub(crate) fn access(&self, item: &Accessory) -> Access {
        item.access(self.bought.contains(&item.id), self.level, |b| {
            self.badges.iter().any(|x| x == b)
        })
    }

    /// The first thing worn that the player may not wear yet.
    fn blocker(&self, character: &Character) -> Option<(&'static Accessory, Access)> {
        character.accessories.iter().find_map(|w| {
            let item = accessory(&w.id)?;
            match self.access(item) {
                Access::Owned => None,
                other => Some((item, other)),
            }
        })
    }
}

/// Words for what stands between a player and an accessory.
pub(crate) fn access_note(access: &Access) -> String {
    match access {
        Access::Owned => "Yours".to_owned(),
        Access::Buy(price) => format!("{} shells", thousands(*price)),
        Access::Level(at) => format!("Reach level {at}"),
        Access::Badge(id) => format!(
            "Earn the {} badge",
            Content::get().badge(id).map_or(id.as_str(), |b| b.name.as_str())
        ),
    }
}

/// Answers a click on an editor that is no longer the live one.
async fn expired(cx: &Cx) -> anyhow::Result<()> {
    cx.reply_ephemeral("This character editor has expired. Run `/character` to open a fresh one.")
        .await
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
    let Some(mut draft) = load(&cx.app, user).await?.filter(|d| Some(d.session) == session) else {
        return expired(cx).await;
    };
    let content = Content::get();
    let value = values.first().map(String::as_str);
    let mut character = draft.character();
    let mut note = String::new();
    match action {
        "tab" => match value.and_then(Tab::from_key) {
            Some(tab) => draft.tab = tab,
            None => return expired(cx).await,
        },
        "slot" => match args.next().and_then(Slot::from_key) {
            Some(slot) => {
                draft.tab = Tab::Gear;
                draft.slot = slot;
            }
            None => return expired(cx).await,
        },
        "prev" | "next" => {
            let forward = action == "next";
            match draft.tab {
                Tab::Look => {
                    let ids: Vec<&str> = content.looks.iter().map(|l| l.id.as_str()).collect();
                    if let Some(id) = step(&ids, Some(&character.look.as_str()), forward) {
                        character.look = id.to_owned();
                    }
                }
                Tab::Paint(channel) => {
                    let swatches: Vec<Paint> = (0..channel.swatches().len()).map(|i| Paint::Swatch(i as u8)).collect();
                    let current = character.colors.get(channel);
                    if let Some(p) = step(&swatches, Some(&current), forward) {
                        character.colors.set(channel, p);
                    }
                }
                Tab::Gear => {
                    let worn = character
                        .worn(draft.slot)
                        .and_then(|w| accessory(&w.id))
                        .map(|a| a.id.as_str());
                    let choices = slot_choices(draft.slot);
                    if let Some(next) = step(&choices, Some(&worn), forward) {
                        character.wear(draft.slot, next, None);
                    }
                }
                Tab::Presets => {
                    let codes: Vec<&str> = presets().iter().map(|p| p.code.as_str()).collect();
                    if let Some(code) = step(&codes, Some(&draft.code.as_str()), forward)
                        && let Some(c) = Character::decode(content, code)
                    {
                        character = c;
                    }
                }
                Tab::Outfits => {
                    let outfits = cx.app.db.outfits(user).await?;
                    let codes: Vec<String> = outfits.iter().map(|o| o.code.clone()).collect();
                    match step(&codes, Some(&draft.code), forward).and_then(|c| Character::decode(content, &c)) {
                        Some(c) => character = c,
                        None => note = "No saved outfits yet: save this one with the buttons below.".to_owned(),
                    }
                }
            }
        }
        "pick" => {
            let Some(value) = value else {
                return expired(cx).await;
            };
            match draft.tab {
                Tab::Look => match content.look(value) {
                    Some(l) => character.look = l.id.clone(),
                    None => return expired(cx).await,
                },
                Tab::Paint(channel) => {
                    // "custom" is the colour already typed in; picking it again changes nothing.
                    if value != "custom" {
                        match Paint::parse(value, channel.swatches().len()) {
                            Some(p @ Paint::Swatch(_)) => character.colors.set(channel, p),
                            _ => return expired(cx).await,
                        }
                    }
                }
                Tab::Gear => {
                    let id = (value != "none").then_some(value);
                    if id.is_some_and(|id| accessory(id).is_none_or(|a| a.slot != draft.slot)) {
                        return expired(cx).await;
                    }
                    let tint = character
                        .worn(draft.slot)
                        .filter(|w| Some(w.id.as_str()) == id)
                        .and_then(|w| w.tint);
                    character.wear(draft.slot, id, tint);
                }
                Tab::Presets => match preset(value).and_then(|p| p.character(content)) {
                    Some(c) => {
                        note = format!(
                            "**{}** is on. Make it your own, then Save.",
                            preset(value).map_or("", |p| &p.name)
                        );
                        character = c;
                    }
                    None => return expired(cx).await,
                },
                Tab::Outfits => {
                    let slot: i16 = value.parse().unwrap_or(0);
                    let outfits = cx.app.db.outfits(user).await?;
                    match outfits.iter().find(|o| o.slot == slot) {
                        Some(o) => match Character::decode(content, &o.code) {
                            Some(c) => {
                                note = format!("Wearing your outfit **{}**.", o.name);
                                character = c;
                            }
                            None => note = "That outfit no longer reads; save a new one over it.".to_owned(),
                        },
                        None => {
                            note = format!(
                                "Outfit slot {slot} is empty: press **Keep in {slot}** to save this look there."
                            )
                        }
                    }
                }
            }
        }
        "undo" => match draft.undo.pop() {
            Some(code) => {
                draft.code = code;
                character = draft.character();
                note = "Undone.".to_owned();
            }
            None => note = "Nothing to undo.".to_owned(),
        },
        "rand" => {
            let shelf = Shelf::load(&cx.app, user).await?;
            character = randomise(
                content,
                &shelf,
                &mut StdRng::seed_from_u64(rand::random_range(0..u64::MAX)),
            );
            note = "Randomised from everything you own. Again, or Undo to go back.".to_owned();
        }
        "tint" => {
            let Some(worn) = character.worn(draft.slot).cloned() else {
                return expired(cx).await;
            };
            let tints: Vec<Option<Paint>> = std::iter::once(None)
                .chain((0..ACCESSORY_TINTS.len()).map(|i| Some(Paint::Swatch(i as u8))))
                .collect();
            let next = step(&tints, Some(&worn.tint), true).flatten();
            character.wear(draft.slot, Some(&worn.id), next);
        }
        "buy" => {
            let shelf = Shelf::load(&cx.app, user).await?;
            let Some(item) = character.worn(draft.slot).and_then(|w| accessory(&w.id)) else {
                return expired(cx).await;
            };
            note = match shelf.access(item) {
                Access::Buy(price) => match cx.app.db.buy_accessory(user, &item.id, price).await? {
                    CustomizeBuy::Bought { balance } => format!(
                        "Bought **{}** for {} shells ({} left). It's yours to wear.",
                        item.name,
                        thousands(price),
                        thousands(balance)
                    ),
                    CustomizeBuy::AlreadyOwned => format!("You already own **{}**.", item.name),
                    CustomizeBuy::Insufficient { balance } => format!(
                        "**{}** costs {} shells; you have {}.",
                        item.name,
                        thousands(price),
                        thousands(balance)
                    ),
                },
                Access::Owned => format!("You already own **{}**.", item.name),
                other => format!("**{}** is locked: {}.", item.name, access_note(&other).to_lowercase()),
            };
        }
        "save" => {
            let shelf = Shelf::load(&cx.app, user).await?;
            note = match shelf.blocker(&character) {
                Some((item, access)) => format!(
                    "Not saved: **{}** isn't yours yet ({}). Buy it or take it off first.",
                    item.name,
                    access_note(&access).to_lowercase()
                ),
                None => {
                    cx.app.db.set_character(user, &draft.code).await?;
                    draft.saved = draft.code.clone();
                    "Saved! Your character stands in your house, joins you on visits and shows on your \
                     profile and in hugs."
                        .to_owned()
                }
            };
        }
        "cancel" => {
            cx.app.cache.forget(&key(user)).await?;
            let saved = saved_code(&cx.app, user).await?;
            let data = view::closed(cx, saved.as_deref()).await?;
            return cx.update(data).await;
        }
        "keep" => {
            let Some(slot) = args
                .next()
                .and_then(|s| s.parse::<i16>().ok())
                .filter(|s| (1..=OUTFIT_SLOTS).contains(s))
            else {
                return expired(cx).await;
            };
            cx.app.db.save_outfit(user, slot, &draft.code).await?;
            draft.tab = Tab::Outfits;
            note = format!("Kept this look in outfit slot {slot}. Rename it with **Rename…**.");
        }
        "hex" => {
            let Tab::Paint(channel) = draft.tab else {
                return expired(cx).await;
            };
            let now = pinchora_render::character::display_rgb(channel.swatches(), character.colors.get(channel));
            return cx
                .modal(
                    format!("ca:hexm:{}", draft.session),
                    &format!("{} colour", channel.label()),
                    vec![text_field(
                        HEX_FIELD,
                        "Hex colour code",
                        "#ff6b4a",
                        7,
                        now.map(|v| format!("#{v:06x}")),
                    )],
                )
                .await;
        }
        "name" => {
            let outfits = cx.app.db.outfits(user).await?;
            if outfits.is_empty() {
                note = "Keep an outfit in a slot first, then name it.".to_owned();
            } else {
                let fields = outfits
                    .iter()
                    .map(|o| {
                        text_field(
                            &format!("slot{}", o.slot),
                            &format!("Outfit {}", o.slot),
                            "Beach day",
                            OUTFIT_NAME_MAX as u16,
                            Some(o.name.clone()),
                        )
                    })
                    .collect();
                return cx
                    .modal(format!("ca:namem:{}", draft.session), "Name your outfits", fields)
                    .await;
            }
        }
        _ => return expired(cx).await,
    }
    if draft.change(&character) && note.is_empty() {
        note = describe(&draft, &character);
    }
    store(&cx.app, user, &draft).await?;
    let data = view::editor(cx, &draft, &note).await?;
    cx.update(data).await
}

/// A one-line note on what the open tab now shows.
fn describe(draft: &Draft, character: &Character) -> String {
    let content = Content::get();
    match draft.tab {
        Tab::Look => format!(
            "Look: **{}**.",
            content
                .look(&character.look)
                .map_or(character.look.as_str(), |l| l.name.as_str())
        ),
        Tab::Paint(channel) => format!(
            "{}: **{}**.",
            channel.label(),
            channel.name_of(character.colors.get(channel))
        ),
        Tab::Gear => match character.worn(draft.slot).and_then(|w| accessory(&w.id)) {
            Some(item) => format!("{}: **{}**.", draft.slot.label(), item.name),
            None => format!("{}: nothing.", draft.slot.label()),
        },
        Tab::Presets | Tab::Outfits => String::new(),
    }
}

/// A random character from what the player may wear: any look and swatch, and in each slot
/// either nothing or something they own.
pub(crate) fn randomise(content: &Content, shelf: &Shelf, rng: &mut impl RngExt) -> Character {
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
        return cx
            .reply_ephemeral("This form has expired. Run `/character` to open a fresh editor.")
            .await;
    };
    let field = |id: &str| fields.iter().find(|(k, _)| k == id).map(|(_, v)| v.trim());
    let note;
    match action {
        "hexm" => {
            let Tab::Paint(channel) = draft.tab else {
                return cx.reply_ephemeral("This form has expired.").await;
            };
            match field(HEX_FIELD).and_then(parse_hex) {
                Some(typed) => {
                    let kept = shadeable(typed);
                    let mut character = draft.character();
                    character.colors.set(channel, Paint::Custom(kept));
                    draft.change(&character);
                    note = if kept == typed {
                        format!("{}: **#{typed:06x}**.", channel.label())
                    } else {
                        format!(
                            "{}: **#{kept:06x}**. `#{typed:06x}` was nudged a little so its light and shade still show.",
                            channel.label()
                        )
                    };
                }
                None => note = "That is not a colour code. Type six hex digits such as `#ff6b4a`.".to_owned(),
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
            note = format!("Named {renamed} outfit{}.", if renamed == 1 { "" } else { "s" });
        }
        _ => return cx.reply_ephemeral("This form has expired.").await,
    }
    store(&cx.app, user, &draft).await?;
    let data = view::editor(cx, &draft, &note).await?;
    cx.update(data).await
}

#[cfg(test)]
mod tests {
    use super::*;

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
