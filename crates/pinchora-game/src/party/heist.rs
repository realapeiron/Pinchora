//! The treasure heist: a crew of crabs against a sunken vault. Three stages, each helped by one role.
//! The vault is a crew-against-the-sea game: nobody ever loses shells, the loot is simply not there when
//! the crew fumbles.

use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;

use crate::party::Rng;

/// Crew members a heist holds, and the least it needs to set off.
pub const MAX_CREW: i64 = 8;
pub const MIN_CREW: usize = 2;
/// Seconds the crew gathers for.
pub const LOBBY_SECS: i64 = 90;
/// Seconds a player waits before joining another heist.
pub const COOLDOWN_SECS: u64 = 300;
/// Shells the system pays one player per day across all heists.
pub const DAILY_SHELLS: i64 = 150;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Lookout,
    Locksmith,
    Diver,
}

impl Role {
    pub const ALL: [Role; 3] = [Self::Lookout, Self::Locksmith, Self::Diver];

    /// The number kept in custom ids and the database.
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|r| *r == self).unwrap_or(0)
    }

    pub fn from_index(index: i64) -> Option<Self> {
        usize::try_from(index).ok().and_then(|i| Self::ALL.get(i).copied())
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Lookout => "Lookout",
            Self::Locksmith => "Locksmith",
            Self::Diver => "Diver",
        }
    }

    /// What the catalogs call the role (`heist.role.<key>`).
    fn key(self) -> &'static str {
        match self {
            Self::Lookout => "lookout",
            Self::Locksmith => "locksmith",
            Self::Diver => "diver",
        }
    }

    /// [`Role::label`] in `lang`.
    pub fn label_in(self, lang: Lang) -> String {
        t!(lang, &format!("heist.role.{}", self.key()))
    }

    /// [`Role::job`] in `lang`.
    pub fn job_in(self, lang: Lang) -> String {
        t!(lang, &format!("heist.job.{}", self.key()))
    }

    /// The role's short word on the card, in capitals.
    pub fn card_tag(self, lang: Lang) -> String {
        t!(lang, &format!("heist.card.{}", self.key()))
    }

    pub fn emoji(self) -> &'static str {
        match self {
            Self::Lookout => "🔭",
            Self::Locksmith => "🔑",
            Self::Diver => "🤿",
        }
    }

    /// What the role does for the crew.
    pub fn job(self) -> &'static str {
        match self {
            Self::Lookout => "keeps the guard crabs looking the other way",
            Self::Locksmith => "cracks the clam lock",
            Self::Diver => "hauls the loot to the surface",
        }
    }
}

/// The three stages of a heist, in order; each is helped by the role of the same number.
pub const STAGES: [&str; 3] = ["Slip past the guards", "Crack the clam lock", "Haul out the loot"];

/// The title of stage `stage` in `lang`.
pub fn stage_title(lang: Lang, stage: usize) -> String {
    t!(lang, &format!("heist.stage.{stage}"))
}

/// How well the crew does at one stage, in whole percent: a base chance, a big bonus for the first
/// member who has the role and smaller ones for more of them, and a little for any extra hands.
pub fn chance(roles: &[Role], stage: usize) -> u8 {
    let Some(&role) = Role::ALL.get(stage) else {
        return 0;
    };
    let specialists = roles.iter().filter(|r| **r == role).count();
    let helpers = roles.len() - specialists;
    let bonus = match specialists {
        0 => 0,
        1 => 30,
        2 => 40,
        _ => 45,
    };
    (30 + bonus + 2 * helpers.min(8)).min(90) as u8
}

/// How a heist went, by how many stages the crew got through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// All three: the vault is open.
    Vault,
    /// Two: a good haul.
    Haul,
    /// One: pocket change.
    Scraps,
    /// None: caught.
    Busted,
}

impl Tier {
    pub fn of(passed: usize) -> Self {
        match passed {
            3.. => Self::Vault,
            2 => Self::Haul,
            1 => Self::Scraps,
            _ => Self::Busted,
        }
    }

    /// Shells each crew member's share is worth, before the daily limit.
    pub fn each(self) -> i64 {
        match self {
            Self::Vault => 60,
            Self::Haul => 25,
            Self::Scraps => 8,
            Self::Busted => 0,
        }
    }

    pub fn headline(self) -> &'static str {
        match self {
            Self::Vault => "THE VAULT IS OPEN",
            Self::Haul => "A GOOD HAUL",
            Self::Scraps => "SCRAPS ONLY",
            Self::Busted => "BUSTED",
        }
    }

    /// [`Tier::headline`] in `lang`, in capitals.
    pub fn headline_in(self, lang: Lang) -> String {
        let key = match self {
            Self::Vault => "vault",
            Self::Haul => "haul",
            Self::Scraps => "scraps",
            Self::Busted => "busted",
        };
        t!(lang, &format!("heist.headline.{key}"))
    }
}

/// What happened at one stage.
#[derive(Debug, Clone)]
pub struct Stage {
    /// The crew's chance, in percent.
    pub chance: u8,
    pub passed: bool,
    pub line: &'static str,
    /// Which stage this is and which of its lines was picked, for [`Stage::line_in`].
    stage: usize,
    pick: usize,
}

impl Stage {
    /// [`Stage::line`] in `lang`.
    pub fn line_in(&self, lang: Lang) -> String {
        let how = if self.passed { "pass" } else { "fail" };
        t!(lang, &format!("heist.line.{how}.{}.{}", self.stage, self.pick))
    }
}

#[derive(Debug, Clone)]
pub struct Heist {
    pub stages: [Stage; 3],
    pub tier: Tier,
}

const PASS_LINES: [[&str; 3]; 3] = [
    [
        "The guard crabs argue about lunch and never look up.",
        "A well-timed bubble screen hides the whole crew.",
        "The lookout whistles once; the guards wander off.",
    ],
    [
        "Click, click, clunk: the clam lock yawns open.",
        "The locksmith hums a lullaby and the lock falls asleep.",
        "One tiny claw, one perfect twist, and the door gives.",
    ],
    [
        "The treasure rides a kelp lift all the way up.",
        "Bag after bag floats out on a trail of bubbles.",
        "The diver kicks off and the loot comes home.",
    ],
];

const FAIL_LINES: [[&str; 3]; 3] = [
    [
        "A guard crab sneezes and the whole patrol turns round.",
        "The crew squeaks across the sand. Every claw in the reef hears it.",
        "Somebody steps on a squeaky sponge.",
    ],
    [
        "The lock snaps shut on a claw and rings the alarm.",
        "Wrong key. Wrong key. Also the wrong lock.",
        "The clam lock decides it has had enough of visitors.",
    ],
    [
        "The bags are heavier than they look and sink straight back.",
        "A curious octopus sits on the loot and will not move.",
        "The kelp lift snaps with a sad little twang.",
    ],
];

/// Plays the heist out for a crew with these `roles`, from `seed`.
pub fn run(seed: u64, roles: &[Role]) -> Heist {
    let mut rng = Rng::new(seed);
    let stages = std::array::from_fn(|stage| {
        let chance = chance(roles, stage);
        let passed = rng.below(100) < u32::from(chance);
        let lines = if passed { &PASS_LINES[stage] } else { &FAIL_LINES[stage] };
        let pick = rng.below(lines.len() as u32) as usize;
        Stage {
            chance,
            passed,
            line: lines[pick],
            stage,
            pick,
        }
    });
    let tier = Tier::of(stages.iter().filter(|s| s.passed).count());
    Heist { stages, tier }
}

#[cfg(test)]
mod tests {
    use crate::party::heist::*;
    use Role::{Diver, Locksmith, Lookout};

    #[test]
    fn a_role_helps_its_own_stage_most() {
        let crew = [Lookout, Locksmith, Diver];
        for stage in 0..3 {
            assert_eq!(chance(&crew, stage), 30 + 30 + 4, "stage {stage}");
        }
        assert_eq!(chance(&[Diver, Diver], 0), 34, "no lookout, two helpers");
        assert_eq!(chance(&[Lookout, Lookout, Lookout, Lookout], 0), 75);
        assert_eq!(chance(&[Lookout, Lookout], 0), 70);
        assert_eq!(chance(&[], 1), 30);
        assert_eq!(chance(&[Lookout], 7), 0, "there is no fourth stage");
        let huge = vec![Lookout; 20];
        assert!(chance(&huge, 0) <= 90);
    }

    #[test]
    fn a_covered_crew_beats_a_crew_of_one_role() {
        let covered = [Lookout, Locksmith, Diver, Lookout];
        let lopsided = [Diver, Diver, Diver, Diver];
        let total = |crew: &[Role]| (0..3).map(|s| u32::from(chance(crew, s))).sum::<u32>();
        assert!(total(&covered) > total(&lopsided) + 40);
    }

    #[test]
    fn every_line_and_label_reads_the_same_in_english_and_is_translated() {
        for stage in 0..3 {
            assert_eq!(stage_title(Lang::En, stage), STAGES[stage]);
            for (how, lines) in [("pass", &PASS_LINES[stage]), ("fail", &FAIL_LINES[stage])] {
                for (pick, line) in lines.iter().enumerate() {
                    let key = format!("heist.line.{how}.{stage}.{pick}");
                    assert_eq!(t!(Lang::En, &key), *line, "{key}");
                    assert_ne!(t!(Lang::Tr, &key), key, "{key} is translated");
                }
            }
        }
        for role in Role::ALL {
            assert_eq!(
                (role.label_in(Lang::En), role.job_in(Lang::En)),
                (role.label().to_owned(), role.job().to_owned())
            );
            assert_ne!(role.label_in(Lang::Tr), role.label());
        }
        for tier in [Tier::Vault, Tier::Haul, Tier::Scraps, Tier::Busted] {
            assert_eq!(tier.headline_in(Lang::En), tier.headline());
        }
    }

    #[test]
    fn the_same_seed_plays_the_same_heist() {
        let crew = [Lookout, Locksmith, Diver];
        let (a, b) = (run(11, &crew), run(11, &crew));
        assert_eq!(a.tier, b.tier);
        for (x, y) in a.stages.iter().zip(&b.stages) {
            assert_eq!((x.chance, x.passed, x.line), (y.chance, y.passed, y.line));
        }
    }

    #[test]
    fn the_odds_match_the_chances_and_the_loot_stays_small() {
        // A full crew of three: each stage 64 %.
        let crew = [Lookout, Locksmith, Diver];
        let runs = 20_000u64;
        let loot: i64 = (0..runs)
            .map(|seed| run(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15), &crew).tier.each())
            .sum();
        let average = loot as f64 / runs as f64;
        assert!(
            (26.0..32.0).contains(&average),
            "a covered trio takes {average:.1} shells each on average"
        );
        // Even the best crew cannot beat the per-player daily limit by much in one sitting.
        let big = [Lookout, Lookout, Locksmith, Locksmith, Diver, Diver, Diver, Lookout];
        let best: i64 = (0..runs)
            .map(|s| run(s.wrapping_mul(0x9e37_79b9_7f4a_7c15), &big).tier.each())
            .sum();
        assert!(((best as f64 / runs as f64) as i64) < Tier::Vault.each());
    }

    #[test]
    fn the_tiers_pay_down_to_nothing() {
        assert_eq!(Tier::of(3), Tier::Vault);
        assert_eq!(Tier::of(2), Tier::Haul);
        assert_eq!(Tier::of(1), Tier::Scraps);
        assert_eq!(Tier::of(0), Tier::Busted);
        assert!(Tier::Vault.each() > Tier::Haul.each() && Tier::Haul.each() > Tier::Scraps.each());
        assert_eq!(Tier::Busted.each(), 0);
    }

    #[test]
    fn roles_round_trip_through_their_number() {
        for role in Role::ALL {
            assert_eq!(Role::from_index(role.index() as i64), Some(role));
        }
        assert_eq!(Role::from_index(3), None);
        assert_eq!(Role::from_index(-1), None);
    }
}
