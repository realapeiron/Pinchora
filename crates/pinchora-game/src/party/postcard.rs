//! Caption the Postcard: a pixel postcard comes up, everyone writes a caption for it, and the table
//! votes for the funniest. The scenes are profile backdrops with pets, fish and characters standing
//! about on them; the picture is drawn from the sprite atlas, so a scene is only a list of places.

use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;

use crate::party::Dice;

/// Rounds a game plays unless the host picks another number, and the range they may pick from.
pub const DEFAULT_ROUNDS: i32 = 3;
pub const ROUNDS: std::ops::RangeInclusive<i32> = 1..=5;
/// Seconds to write a caption, and to vote on them.
pub const WRITE_SECS: i64 = 60;
pub const VOTE_SECS: i64 = 40;
/// How long the results of a round stay up before the next one.
pub const REVEAL_SECS: i64 = 12;
/// Shortest and longest caption, in characters.
pub const MIN_CAPTION: usize = 3;
pub const MAX_CAPTION: usize = 90;

/// What a prop is drawn from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Art {
    /// A pet species (`p_<id>` in the atlas).
    Pet(&'static str),
    /// A fish or a piece of junk from the fishing pond (`c_<id>`).
    Fish(&'static str),
    /// A character preset (`content/character_presets.json`).
    Hero(&'static str),
}

/// One thing standing in a scene: `x` is the middle of its feet and `y` the row they stand on, in the
/// 240×135 pixels of the backdrop.
#[derive(Debug, Clone, Copy)]
pub struct Prop {
    pub art: Art,
    pub x: i32,
    pub y: i32,
    pub flip: bool,
}

/// Every prop is drawn as the atlas has it, one sprite pixel to one backdrop pixel: a bigger one would show
/// blocks of another size next to the rest of the scene.
const fn prop(art: Art, x: i32, y: i32, flip: bool) -> Prop {
    Prop { art, x, y, flip }
}

/// A postcard: a backdrop (`bg_<id>` in the atlas), the place it greets from, and who is in it.
#[derive(Debug, Clone, Copy)]
pub struct Scene {
    pub id: &'static str,
    pub place: &'static str,
    pub backdrop: &'static str,
    pub props: &'static [Prop],
}

use Art::{Fish, Hero, Pet};

/// Every postcard there is.
pub const SCENES: &[Scene] = &[
    Scene {
        id: "dunes_stand_off",
        place: "Driftwood Dunes",
        backdrop: "driftwood_dunes",
        props: &[
            prop(Pet("drowsepatch_cat"), 78, 118, false),
            prop(Fish("snaggletoe_sock"), 122, 118, false),
            prop(Pet("pebblepouch_hermit"), 166, 118, true),
        ],
    },
    Scene {
        id: "noon_umbrella",
        place: "Tideglass Noon",
        backdrop: "tideglass_noon",
        props: &[
            prop(Hero("beach_surfer"), 112, 122, false),
            prop(Pet("puddlesock_otter"), 154, 124, true),
            prop(Pet("pearlwhisk_seal"), 205, 124, true),
        ],
    },
    Scene {
        id: "shore_argument",
        place: "Shellwash Shore",
        backdrop: "shellwash_morning",
        props: &[
            prop(Pet("crumbclaw_crab"), 84, 120, false),
            prop(Fish("siltstep_boot"), 124, 122, false),
            prop(Pet("sprinklearm_octopus"), 168, 120, true),
        ],
    },
    Scene {
        id: "bluff_picnic_line",
        place: "Clovercap Bluff",
        backdrop: "clovercap_bluff",
        props: &[
            prop(Pet("cloverpuff_bun"), 60, 112, false),
            prop(Pet("quiltquill_hedgehog"), 100, 118, false),
            prop(Hero("trail_scout"), 150, 122, true),
            prop(Pet("tumblebutton_pup"), 196, 124, true),
        ],
    },
    Scene {
        id: "boardwalk_big_one",
        place: "Misty Boardwalk",
        backdrop: "misty_boardwalk",
        props: &[
            prop(Hero("ship_captain"), 84, 118, false),
            prop(Fish("plumspot_puffer"), 140, 120, false),
            prop(Pet("drowsepatch_cat"), 190, 120, true),
        ],
    },
    Scene {
        id: "jetty_night_shift",
        place: "Starlit Jetty",
        backdrop: "starlit_jetty",
        props: &[
            prop(Hero("moon_bat"), 96, 120, false),
            prop(Pet("hoothem_owlet"), 140, 120, true),
            prop(Fish("hushbell_angler"), 176, 118, true),
        ],
    },
    Scene {
        id: "market_fish_deal",
        place: "Brinebell Market",
        backdrop: "brinebell_market",
        props: &[
            prop(Hero("sweet_tooth"), 90, 124, false),
            prop(Fish("reefruff_lionfish"), 134, 124, false),
            prop(Pet("tasselcrest_cockatiel"), 180, 124, true),
        ],
    },
    Scene {
        id: "firefly_club",
        place: "Firefly Picnic",
        backdrop: "firefly_picnic",
        props: &[
            prop(Hero("coral_witch"), 80, 118, false),
            prop(Pet("rufflebranch_axolotl"), 124, 122, false),
            prop(Pet("teacup_toadlet"), 160, 124, true),
        ],
    },
    Scene {
        id: "cove_pirates",
        place: "Biscuit Cove",
        backdrop: "biscuit_cove",
        props: &[
            prop(Hero("reef_pirate"), 100, 124, false),
            prop(Pet("chirrupfold_budgie"), 140, 124, true),
            prop(Pet("lanternclasp_crab"), 172, 124, true),
        ],
    },
    Scene {
        id: "snow_day_off",
        place: "Snowmuffin Village",
        backdrop: "snowmuffin_village",
        props: &[
            prop(Hero("snow_day"), 96, 122, false),
            prop(Pet("quiltquill_hedgehog"), 140, 124, true),
            prop(Pet("acornpocket_squirrel"), 178, 124, true),
        ],
    },
    Scene {
        id: "oasis_slow_lane",
        place: "Palmcup Oasis",
        backdrop: "palmcup_oasis",
        props: &[
            prop(Pet("wishroot_tortoise"), 84, 124, false),
            prop(Pet("dunetuft_fennec"), 128, 124, true),
            prop(Fish("tidefold_locket"), 160, 124, true),
        ],
    },
];

pub fn scene(id: &str) -> Option<&'static Scene> {
    SCENES.iter().find(|s| s.id == id)
}

/// A scene by the dice that is not one of `seen` (the ones this game already showed), when there is one.
pub fn pick_scene(dice: &mut Dice, seen: &[&str]) -> &'static Scene {
    let fresh: Vec<&Scene> = SCENES.iter().filter(|s| !seen.contains(&s.id)).collect();
    if fresh.is_empty() {
        return &SCENES[dice.below(SCENES.len())];
    }
    fresh[dice.below(fresh.len())]
}

/// Why a caption did not count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptionError {
    TooShort,
    TooLong,
}

impl CaptionError {
    /// What to tell the player, in English.
    pub fn say(self) -> String {
        self.say_in(Lang::En)
    }

    /// The same in `lang` (`postcard.err.*` in the catalogs).
    pub fn say_in(self, lang: Lang) -> String {
        match self {
            Self::TooShort => t!(lang, "postcard.err.too-short", min = MIN_CAPTION),
            Self::TooLong => t!(lang, "postcard.err.too-long", max = MAX_CAPTION),
        }
    }
}

/// A caption with its spaces tidied, if it is the right length.
pub fn check_caption(text: &str) -> Result<String, CaptionError> {
    let caption = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match caption.chars().count() {
        n if n < MIN_CAPTION => Err(CaptionError::TooShort),
        n if n > MAX_CAPTION => Err(CaptionError::TooLong),
        _ => Ok(caption),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::party::postcard::*;
    use crate::{character, content::Content};

    #[test]
    fn every_scene_is_made_of_real_art() {
        let atlas = pinchora_render::character::atlas();
        let pets: HashSet<&str> = Content::get().pets.iter().map(|p| p.id.as_str()).collect();
        let fish: HashSet<&str> = Content::get().fish.iter().map(|f| f.id.as_str()).collect();
        let mut ids = HashSet::new();
        assert!(SCENES.len() >= 10);
        for scene in SCENES {
            assert!(ids.insert(scene.id), "duplicate scene {}", scene.id);
            assert!(
                atlas.get(&format!("bg_{}", scene.backdrop)).is_some(),
                "{}",
                scene.backdrop
            );
            assert!(
                (2..=4).contains(&scene.props.len()),
                "{} has {} props",
                scene.id,
                scene.props.len()
            );
            assert!(!scene.place.is_empty() && scene.place.len() <= 24, "{}", scene.place);
            for prop in scene.props {
                assert!(
                    (0..240).contains(&prop.x) && (0..=135).contains(&prop.y),
                    "{}",
                    scene.id
                );
                match prop.art {
                    Art::Pet(id) => assert!(pets.contains(id), "{id}"),
                    Art::Fish(id) => assert!(fish.contains(id), "{id}"),
                    Art::Hero(id) => assert!(character::preset(id).is_some(), "{id}"),
                }
            }
        }
    }

    #[test]
    fn scenes_are_not_repeated_until_they_run_out() {
        let mut dice = Dice::new(3);
        let mut seen: Vec<&str> = Vec::new();
        for _ in 0..SCENES.len() {
            let next = pick_scene(&mut dice, &seen);
            assert!(!seen.contains(&next.id));
            seen.push(next.id);
        }
        // Out of fresh ones, it still shows something.
        assert!(scene(pick_scene(&mut dice, &seen).id).is_some());
        assert!(scene("nope").is_none());
    }

    #[test]
    fn captions_have_limits_and_tidy_spaces() {
        assert_eq!(check_caption("  He   said   no  "), Ok("He said no".to_owned()));
        assert_eq!(check_caption("hi"), Err(CaptionError::TooShort));
        assert_eq!(check_caption("   "), Err(CaptionError::TooShort));
        assert_eq!(check_caption(&"a".repeat(91)), Err(CaptionError::TooLong));
        assert!(check_caption(&"a".repeat(90)).is_ok());
        assert!(CaptionError::TooLong.say().contains("90"));
        assert!(CaptionError::TooLong.say_in(Lang::Tr).contains("90 karakteri"));
        assert!(CaptionError::TooShort.say_in(Lang::Tr).contains("en az 3 karakter"));
    }
}
