//! The hermit crab derby: six crabs, one lane each, a race decided by a seed.
//!
//! Every crab runs the same random process, so each has the same chance of winning and the pool
//! can be shared out between the people who backed the winner without the house taking a cut.

use super::Rng;

/// Lanes on the track.
pub const CRABS: usize = 6;
/// Racers one lobby holds.
pub const MAX_RACERS: i64 = 30;
/// Seconds the lobby stays open for picks.
pub const LOBBY_SECS: i64 = 75;
/// A race with shells at stake needs this many players, so nobody races against themselves.
pub const MIN_STAKED_RACERS: usize = 2;
/// Milliseconds one step of the race lasts in the animation.
pub const TICK_MS: u32 = 90;

/// A racing crab: its name, the coloured square on its button, its lane colour and the three
/// shades (dark, middle, light) its shell is painted in, as `0xrrggbb`.
#[derive(Debug, Clone, Copy)]
pub struct Crab {
    pub name: &'static str,
    pub emoji: &'static str,
    pub accent: u32,
    pub shell: [u32; 3],
}

pub const ROSTER: [Crab; CRABS] = [
    Crab {
        name: "Pebble",
        emoji: "🟥",
        accent: 0xff6b4a,
        shell: [0x9a2f33, 0xe04a3f, 0xff8a6a],
    },
    Crab {
        name: "Scuttle",
        emoji: "🟦",
        accent: 0x3a9bdc,
        shell: [0x1f5f8b, 0x3a9bdc, 0x8fd3ff],
    },
    Crab {
        name: "Kelp",
        emoji: "🟩",
        accent: 0x06d6a0,
        shell: [0x0b5e4c, 0x06b58a, 0x6ff0c8],
    },
    Crab {
        name: "Barnacle",
        emoji: "🟨",
        accent: 0xffd166,
        shell: [0x8a5a10, 0xe3a21e, 0xffd166],
    },
    Crab {
        name: "Clawdia",
        emoji: "🟪",
        accent: 0xb48ae0,
        shell: [0x4b2f6b, 0x7d55a3, 0xc2a1ea],
    },
    Crab {
        name: "Tidbit",
        emoji: "🟧",
        accent: 0xf08a24,
        shell: [0xa84b13, 0xf08a24, 0xffc27a],
    },
];

/// How far a crab walks in one step when nothing special happens: the track in `1 / BASE` steps.
const BASE: f32 = 1.0 / 56.0;
/// Steps after the winner crosses that the animation still runs, so the others can come in.
const TAIL: usize = 14;
/// A race never runs longer than this, whatever the crabs do.
const MAX_STEPS: usize = 220;

/// A finished race: where each crab was at every step, and who came in when.
#[derive(Debug, Clone)]
pub struct Race {
    /// Progress at every step: 0.0 is the start line, 1.0 the finish line; crabs past the line keep
    /// counting up, and the animation stops them there.
    pub frames: Vec<[f32; CRABS]>,
    /// Crab numbers from first across the line to last. A crab still on the track when the
    /// animation ends is placed by how far it got.
    pub order: [usize; CRABS],
}

impl Race {
    pub fn winner(&self) -> usize {
        self.order[0]
    }

    /// The place (0 is first) a crab came in.
    pub fn place(&self, crab: usize) -> usize {
        self.order.iter().position(|&c| c == crab).unwrap_or(CRABS - 1)
    }
}

#[derive(Clone, Copy, Default)]
struct Runner {
    pos: f32,
    /// Slow drift in pace, which pulls back to zero.
    mood: f32,
    burst: u8,
    stall: u8,
    /// The moment (in steps, fractional) the crab crossed the line.
    crossed: Option<f32>,
}

/// Runs the race for `seed`. The same seed always runs the same race.
pub fn run(seed: u64) -> Race {
    let mut rng = Rng::new(seed);
    let mut runners = [Runner::default(); CRABS];
    // Settles the (all but impossible) photo finish between two crabs at the very same instant.
    let tiebreak: [u64; CRABS] = std::array::from_fn(|_| rng.next_u64());
    let mut frames = vec![[0.0; CRABS]];
    let mut winner_step = None;
    for step in 1..=MAX_STEPS {
        for runner in &mut runners {
            let speed = if runner.burst > 0 {
                runner.burst -= 1;
                BASE * 1.9
            } else if runner.stall > 0 {
                runner.stall -= 1;
                BASE * 0.15
            } else {
                runner.mood = runner.mood * 0.8 + rng.between(-0.35, 0.35);
                if rng.chance(0.07) {
                    runner.burst = 2 + rng.below(3) as u8;
                } else if rng.chance(0.05) {
                    runner.stall = 1 + rng.below(3) as u8;
                }
                BASE * (1.0 + runner.mood).clamp(0.45, 1.6)
            };
            let before = runner.pos;
            runner.pos += speed;
            if runner.crossed.is_none() && runner.pos >= 1.0 {
                runner.crossed = Some((step - 1) as f32 + (1.0 - before) / speed);
            }
        }
        frames.push(runners.map(|r| r.pos));
        winner_step = winner_step.or_else(|| runners.iter().any(|r| r.crossed.is_some()).then_some(step));
        let everyone_in = runners.iter().all(|r| r.crossed.is_some());
        if everyone_in || winner_step.is_some_and(|w| step >= w + TAIL) {
            break;
        }
    }
    let mut order: [usize; CRABS] = std::array::from_fn(|i| i);
    order.sort_by(|&a, &b| {
        let (ra, rb) = (&runners[a], &runners[b]);
        match (ra.crossed, rb.crossed) {
            (Some(x), Some(y)) => x.total_cmp(&y).then(tiebreak[a].cmp(&tiebreak[b])),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => rb.pos.total_cmp(&ra.pos).then(tiebreak[a].cmp(&tiebreak[b])),
        }
    });
    Race { frames, order }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seed_runs_the_same_race_every_time() {
        let (a, b) = (run(99), run(99));
        assert_eq!(a.order, b.order);
        assert_eq!(a.frames, b.frames);
        assert_ne!(run(1).frames, run(2).frames);
    }

    #[test]
    fn a_race_is_a_whole_ranking_that_ends_with_the_winner_past_the_line() {
        for seed in 0..300 {
            let race = run(seed);
            let mut seen = race.order;
            seen.sort_unstable();
            assert_eq!(seen, [0, 1, 2, 3, 4, 5], "seed {seed}");
            let last = race.frames.last().expect("frames");
            assert!(last[race.winner()] >= 1.0, "seed {seed}: the winner crossed");
            assert!(
                race.frames.windows(2).all(|w| (0..CRABS).all(|c| w[1][c] >= w[0][c])),
                "seed {seed}: crabs never walk backwards"
            );
            assert!(
                (40..=MAX_STEPS + 1).contains(&race.frames.len()),
                "seed {seed}: {} steps",
                race.frames.len()
            );
            // The place a crab came in follows how far it got when it crossed first.
            assert_eq!(race.place(race.winner()), 0);
        }
    }

    #[test]
    fn every_lane_wins_about_a_sixth_of_the_races() {
        let mut wins = [0u32; CRABS];
        let races = 6_000u32;
        for seed in 0..u64::from(races) {
            wins[run(seed.wrapping_mul(0x9e37_79b9)).winner()] += 1;
        }
        for (lane, won) in wins.iter().enumerate() {
            let share = *won as f32 / races as f32;
            assert!(
                (0.14..0.195).contains(&share),
                "lane {lane} won {share:.3} of the races: {wins:?}"
            );
        }
    }

    #[test]
    fn the_lead_changes_hands_in_a_fair_share_of_races() {
        let changed = (0..400u64)
            .filter(|&seed| {
                let race = run(seed);
                let leader = |f: &[f32; CRABS]| (0..CRABS).max_by(|&a, &b| f[a].total_cmp(&f[b])).unwrap_or(0);
                let mid = race.frames[race.frames.len() / 3];
                leader(&mid) != race.winner()
            })
            .count();
        assert!(changed > 60, "only {changed} of 400 races were not decided early");
    }

    #[test]
    fn the_roster_has_six_distinct_crabs() {
        let mut names: Vec<_> = ROSTER.iter().map(|c| c.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), CRABS);
        let mut accents: Vec<_> = ROSTER.iter().map(|c| c.accent).collect();
        accents.sort_unstable();
        accents.dedup();
        assert_eq!(accents.len(), CRABS);
    }
}
