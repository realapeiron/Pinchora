use super::*;

pub(super) struct CharacterEdit<'a> {
    pub(super) cx: &'a Cx,
    pub(super) user: u64,
    pub(super) draft: Draft,
    pub(super) character: Character,
    pub(super) note: String,
}

impl CharacterEdit<'_> {
    pub(super) async fn select_tab(&mut self, value: Option<&str>) -> anyhow::Result<bool> {
        let cx = self.cx;
        let draft = &mut self.draft;
        match value.and_then(Tab::from_key) {
            Some(tab) => draft.tab = tab,
            None => {
                expired(cx).await?;
                return Ok(false);
            }
        };
        Ok(true)
    }

    pub(super) async fn select_slot(&mut self, args: &mut Split<'_, char>) -> anyhow::Result<bool> {
        let cx = self.cx;
        let draft = &mut self.draft;
        match args.next().and_then(Slot::from_key) {
            Some(slot) => {
                draft.tab = Tab::Gear;
                draft.slot = slot;
            }
            None => {
                expired(cx).await?;
                return Ok(false);
            }
        };
        Ok(true)
    }

    pub(super) async fn step_choice(&mut self, action: &str) -> anyhow::Result<bool> {
        let cx = self.cx;
        let user = self.user;
        let draft = &mut self.draft;
        let character = &mut self.character;
        let note = &mut self.note;
        let content = Content::get();

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
                    *character = c;
                }
            }
            Tab::Outfits => {
                let outfits = cx.app.db.outfits(user).await?;
                let codes: Vec<String> = outfits.iter().map(|o| o.code.clone()).collect();
                match step(&codes, Some(&draft.code), forward).and_then(|c| Character::decode(content, &c)) {
                    Some(c) => *character = c,
                    None => *note = t!(cx.lang(), "character.no-outfits"),
                }
            }
        }
        Ok(true)
    }

    pub(super) async fn pick_choice(&mut self, value: Option<&str>) -> anyhow::Result<bool> {
        let cx = self.cx;
        let user = self.user;
        let draft = &mut self.draft;
        let character = &mut self.character;
        let note = &mut self.note;
        let content = Content::get();

        let Some(value) = value else {
            expired(cx).await?;
            return Ok(false);
        };
        match draft.tab {
            Tab::Look => match content.look(value) {
                Some(l) => character.look = l.id.clone(),
                None => {
                    expired(cx).await?;
                    return Ok(false);
                }
            },
            Tab::Paint(channel) => {
                // "custom" is the colour already typed in; picking it again changes nothing.
                if value != "custom" {
                    match Paint::parse(value, channel.swatches().len()) {
                        Some(p @ Paint::Swatch(_)) => character.colors.set(channel, p),
                        _ => {
                            expired(cx).await?;
                            return Ok(false);
                        }
                    }
                }
            }
            Tab::Gear => {
                let id = (value != "none").then_some(value);
                if id.is_some_and(|id| accessory(id).is_none_or(|a| a.slot != draft.slot)) {
                    expired(cx).await?;
                    return Ok(false);
                }
                let tint = character
                    .worn(draft.slot)
                    .filter(|w| Some(w.id.as_str()) == id)
                    .and_then(|w| w.tint);
                character.wear(draft.slot, id, tint);
            }
            Tab::Presets => match preset(value).and_then(|p| p.character(content)) {
                Some(c) => {
                    *note = t!(
                        cx.lang(),
                        "character.preset-on",
                        name = preset(value).map_or("", |p| &p.name)
                    );
                    *character = c;
                }
                None => {
                    expired(cx).await?;
                    return Ok(false);
                }
            },
            Tab::Outfits => {
                let slot: i16 = value.parse().unwrap_or(0);
                let outfits = cx.app.db.outfits(user).await?;
                match outfits.iter().find(|o| o.slot == slot) {
                    Some(o) => match Character::decode(content, &o.code) {
                        Some(c) => {
                            *note = t!(cx.lang(), "character.wearing-outfit", name = o.name);
                            *character = c;
                        }
                        None => *note = t!(cx.lang(), "character.outfit-broken"),
                    },
                    None => *note = t!(cx.lang(), "character.outfit-empty", slot = slot),
                }
            }
        }
        Ok(true)
    }

    pub(super) async fn undo_change(&mut self) -> anyhow::Result<bool> {
        let cx = self.cx;
        let draft = &mut self.draft;
        let character = &mut self.character;
        let note = &mut self.note;
        match draft.undo.pop() {
            Some(code) => {
                draft.code = code;
                *character = draft.character();
                *note = t!(cx.lang(), "character.undone");
            }
            None => *note = t!(cx.lang(), "character.nothing-to-undo"),
        };
        Ok(true)
    }

    pub(super) async fn randomise_character(&mut self) -> anyhow::Result<bool> {
        let cx = self.cx;
        let user = self.user;
        let character = &mut self.character;
        let note = &mut self.note;
        let content = Content::get();

        let shelf = load_shelf(&cx.app, user).await?;
        *character = randomise(
            content,
            &shelf,
            &mut StdRng::seed_from_u64(rand::random_range(0..u64::MAX)),
        );
        *note = t!(cx.lang(), "character.randomised");
        Ok(true)
    }

    pub(super) async fn tint_accessory(&mut self) -> anyhow::Result<bool> {
        let cx = self.cx;
        let draft = &mut self.draft;
        let character = &mut self.character;

        let Some(worn) = character.worn(draft.slot).cloned() else {
            expired(cx).await?;
            return Ok(false);
        };
        let tints: Vec<Option<Paint>> = std::iter::once(None)
            .chain((0..ACCESSORY_TINTS.len()).map(|i| Some(Paint::Swatch(i as u8))))
            .collect();
        let next = step(&tints, Some(&worn.tint), true).flatten();
        character.wear(draft.slot, Some(&worn.id), next);
        Ok(true)
    }

    pub(super) async fn buy_accessory(&mut self) -> anyhow::Result<bool> {
        let cx = self.cx;
        let user = self.user;
        let draft = &mut self.draft;
        let character = &mut self.character;
        let note = &mut self.note;

        let shelf = load_shelf(&cx.app, user).await?;
        let Some(item) = character.worn(draft.slot).and_then(|w| accessory(&w.id)) else {
            expired(cx).await?;
            return Ok(false);
        };
        *note = match shelf.access(item) {
            Access::Buy(price) => match cx.app.db.buy_accessory(user, &item.id, price).await? {
                CustomizeBuy::Bought { balance } => t!(
                    cx.lang(),
                    "character.bought",
                    item = item.name,
                    price = thousands_in(cx.lang(), price),
                    balance = thousands_in(cx.lang(), balance)
                ),
                CustomizeBuy::AlreadyOwned => t!(cx.lang(), "character.already-own", item = item.name),
                CustomizeBuy::Insufficient { balance } => t!(
                    cx.lang(),
                    "character.costs",
                    item = item.name,
                    price = thousands_in(cx.lang(), price),
                    balance = thousands_in(cx.lang(), balance)
                ),
            },
            Access::Owned => t!(cx.lang(), "character.already-own", item = item.name),
            other => t!(
                cx.lang(),
                "character.locked",
                item = item.name,
                why = access_note(&other, cx.lang()).to_lowercase()
            ),
        };
        Ok(true)
    }

    pub(super) async fn save_character(&mut self) -> anyhow::Result<bool> {
        let cx = self.cx;
        let user = self.user;
        let draft = &mut self.draft;
        let character = &mut self.character;
        let note = &mut self.note;

        let shelf = load_shelf(&cx.app, user).await?;
        *note = match shelf.blocker(character) {
            Some((item, access)) => t!(
                cx.lang(),
                "character.not-saved",
                item = item.name,
                why = access_note(&access, cx.lang()).to_lowercase()
            ),
            None => {
                cx.app.db.set_character(user, &draft.code).await?;
                draft.saved = draft.code.clone();
                t!(cx.lang(), "character.saved")
            }
        };
        Ok(true)
    }

    pub(super) async fn cancel_editor(&mut self) -> anyhow::Result<bool> {
        let cx = self.cx;
        let user = self.user;
        let draft = &mut self.draft;

        cx.app.cache.forget(&key(user)).await?;
        let saved = saved_code(&cx.app, user).await?;
        let data = crate::commands::social::character::view::closed(cx, saved.as_deref(), !draft.is_saved()).await?;
        cx.update(data).await?;
        Ok(false)
    }

    pub(super) async fn keep_outfit(&mut self, args: &mut Split<'_, char>) -> anyhow::Result<bool> {
        let cx = self.cx;
        let user = self.user;
        let draft = &mut self.draft;
        let note = &mut self.note;

        let Some(slot) = args
            .next()
            .and_then(|s| s.parse::<i16>().ok())
            .filter(|s| (1..=OUTFIT_SLOTS).contains(s))
        else {
            expired(cx).await?;
            return Ok(false);
        };
        cx.app.db.save_outfit(user, slot, &draft.code).await?;
        draft.tab = Tab::Outfits;
        *note = t!(cx.lang(), "character.kept", slot = slot);
        Ok(true)
    }

    pub(super) async fn choose_hex(&mut self) -> anyhow::Result<bool> {
        let cx = self.cx;
        let draft = &mut self.draft;
        let character = &mut self.character;

        let Tab::Paint(channel) = draft.tab else {
            expired(cx).await?;
            return Ok(false);
        };
        let now = pinchora_render::character::display_rgb(channel.swatches(), character.colors.get(channel));
        cx.modal(
            format!("ca:hexm:{}", draft.session),
            &t!(
                cx.lang(),
                "character.hex-title",
                channel = channel_name(channel, cx.lang())
            ),
            vec![text_field(
                HEX_FIELD,
                &t!(cx.lang(), "character.hex-label"),
                "#ff6b4a",
                7,
                now.map(|v| format!("#{v:06x}")),
            )],
        )
        .await?;
        Ok(false)
    }

    pub(super) async fn name_outfits(&mut self) -> anyhow::Result<bool> {
        let cx = self.cx;
        let user = self.user;
        let draft = &mut self.draft;
        let note = &mut self.note;

        let outfits = cx.app.db.outfits(user).await?;
        if outfits.is_empty() {
            *note = t!(cx.lang(), "character.name-first");
        } else {
            let fields = outfits
                .iter()
                .map(|o| {
                    text_field(
                        &format!("slot{}", o.slot),
                        &t!(cx.lang(), "character.outfit-slot", slot = o.slot),
                        &t!(cx.lang(), "character.outfit-example"),
                        OUTFIT_NAME_MAX as u16,
                        Some(o.name.clone()),
                    )
                })
                .collect();
            cx.modal(
                format!("ca:namem:{}", draft.session),
                &t!(cx.lang(), "character.name-title"),
                fields,
            )
            .await?;
            return Ok(false);
        }
        Ok(true)
    }
}
