//! Sea Fleet, battleship on a 10×10 sea: fleets, layouts from a seed, and shots.
//!
//! A cell is `y * 10 + x`; columns read A to J along the top and rows 1 to 10 down the side.

pub const SIZE: usize = 10;
pub const CELLS: usize = SIZE * SIZE;

/// The five ships of a fleet and how many cells each takes.
pub const SHIPS: [(&str, usize); 5] = [("Galleon", 5), ("Frigate", 4), ("Brig", 3), ("Sloop", 3), ("Dinghy", 2)];

/// Layouts that are picked by name; any other seed is a random layout.
pub const PRESETS: [(&str, &str); 5] = [
    ("Harbour line", "00h09h92v96v45h"),
    ("Scatter", "11v60h38h84v53v"),
    ("Corners", "59h06v00h90v44h"),
    ("Diagonal", "00v22h45v66h88v"),
    ("Fortress", "20h92v19h03v55h"),
];

/// `A7` for the cell at column 0, row 6.
pub fn cell_name(cell: usize) -> String {
    format!("{}{}", char::from(b'A' + (cell % SIZE) as u8), cell / SIZE + 1)
}

/// Reads `a7`, `A7` or `J10`.
pub fn parse_cell(text: &str) -> Option<usize> {
    let mut chars = text.trim().chars();
    let column = chars.next()?.to_ascii_uppercase();
    let row: usize = chars.as_str().parse().ok()?;
    let x = (column as usize).checked_sub('A' as usize).filter(|&x| x < SIZE)?;
    (1..=SIZE).contains(&row).then(|| (row - 1) * SIZE + x)
}

/// Where one ship lies: its first cell and whether it runs right or down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed {
    pub x: usize,
    pub y: usize,
    pub horizontal: bool,
}

/// A fleet: one [`Placed`] per entry of [`SHIPS`], in that order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fleet {
    pub ships: [Placed; 5],
}

impl Fleet {
    /// The cells ship `index` covers.
    pub fn cells_of(&self, index: usize) -> Vec<usize> {
        let ship = self.ships[index];
        (0..SHIPS[index].1)
            .map(|k| {
                if ship.horizontal {
                    ship.y * SIZE + ship.x + k
                } else {
                    (ship.y + k) * SIZE + ship.x
                }
            })
            .collect()
    }

    /// Which ship lies on each cell, as its index in [`SHIPS`].
    pub fn grid(&self) -> [Option<u8>; CELLS] {
        let mut grid = [None; CELLS];
        for index in 0..SHIPS.len() {
            for cell in self.cells_of(index) {
                grid[cell] = Some(index as u8);
            }
        }
        grid
    }

    /// Whether every ship is inside the sea and no two share a cell.
    pub fn is_valid(&self) -> bool {
        let mut seen = [false; CELLS];
        for (index, (_, len)) in SHIPS.iter().enumerate() {
            let ship = self.ships[index];
            let (end_x, end_y) = if ship.horizontal {
                (ship.x + len - 1, ship.y)
            } else {
                (ship.x, ship.y + len - 1)
            };
            if end_x >= SIZE || end_y >= SIZE {
                return false;
            }
            for cell in self.cells_of(index) {
                if std::mem::replace(&mut seen[cell], true) {
                    return false;
                }
            }
        }
        true
    }

    /// Five ships as three characters each: column digit, row digit, `h` or `v`.
    pub fn encode(&self) -> String {
        self.ships
            .iter()
            .map(|s| format!("{}{}{}", s.x, s.y, if s.horizontal { 'h' } else { 'v' }))
            .collect()
    }

    pub fn decode(text: &str) -> Option<Self> {
        if text.len() != 15 || !text.is_ascii() {
            return None;
        }
        let mut ships = [Placed {
            x: 0,
            y: 0,
            horizontal: true,
        }; 5];
        for (ship, chunk) in ships.iter_mut().zip(text.as_bytes().chunks(3)) {
            *ship = Placed {
                x: usize::from(chunk[0].checked_sub(b'0').filter(|d| *d < 10)?),
                y: usize::from(chunk[1].checked_sub(b'0').filter(|d| *d < 10)?),
                horizontal: match chunk[2] {
                    b'h' => true,
                    b'v' => false,
                    _ => return None,
                },
            };
        }
        let fleet = Self { ships };
        fleet.is_valid().then_some(fleet)
    }

    /// The layout for a seed: a preset for the first few, a random one for every other number. The same
    /// seed always gives the same fleet, so a button can carry the layout it shows.
    pub fn from_seed(seed: u32) -> Self {
        if let Some((_, code)) = PRESETS.get(seed as usize) {
            return Self::decode(code).expect("presets are valid");
        }
        let mut state = u64::from(seed).wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
        let mut next = move |bound: usize| {
            // xorshift64*, plenty for scattering ships.
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            (state.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 33) as usize % bound
        };
        'attempt: loop {
            let mut taken = [false; CELLS];
            let mut ships = [Placed {
                x: 0,
                y: 0,
                horizontal: true,
            }; 5];
            for (index, (_, len)) in SHIPS.iter().enumerate() {
                let mut placed = false;
                for _ in 0..200 {
                    let horizontal = next(2) == 0;
                    let (max_x, max_y) = if horizontal {
                        (SIZE - len + 1, SIZE)
                    } else {
                        (SIZE, SIZE - len + 1)
                    };
                    let ship = Placed {
                        x: next(max_x),
                        y: next(max_y),
                        horizontal,
                    };
                    let cells: Vec<usize> = (0..*len)
                        .map(|k| {
                            if horizontal {
                                ship.y * SIZE + ship.x + k
                            } else {
                                (ship.y + k) * SIZE + ship.x
                            }
                        })
                        .collect();
                    if cells.iter().all(|&c| !taken[c]) {
                        for c in cells {
                            taken[c] = true;
                        }
                        ships[index] = ship;
                        placed = true;
                        break;
                    }
                }
                if !placed {
                    continue 'attempt;
                }
            }
            return Self { ships };
        }
    }
}

/// How a cell of the sea looks to the one shooting at it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shot {
    Miss,
    Hit,
}

/// What a shot did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fired {
    Miss,
    Hit,
    /// The shot sank this ship (an index into [`SHIPS`]).
    Sunk(usize),
    /// That cell was shot before.
    Again,
}

/// The shots fired into one sea.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sea {
    shots: [Option<Shot>; CELLS],
}

impl Default for Sea {
    fn default() -> Self {
        Self::new()
    }
}

impl Sea {
    pub fn new() -> Self {
        Self { shots: [None; CELLS] }
    }

    /// Shots as 100 characters of `.`, `m` (miss) and `h` (hit).
    pub fn encode(&self) -> String {
        self.shots
            .iter()
            .map(|s| match s {
                None => '.',
                Some(Shot::Miss) => 'm',
                Some(Shot::Hit) => 'h',
            })
            .collect()
    }

    pub fn decode(text: &str) -> Option<Self> {
        if text.len() != CELLS {
            return None;
        }
        let mut shots = [None; CELLS];
        for (shot, c) in shots.iter_mut().zip(text.chars()) {
            *shot = match c {
                '.' => None,
                'm' => Some(Shot::Miss),
                'h' => Some(Shot::Hit),
                _ => return None,
            };
        }
        Some(Self { shots })
    }

    pub fn shot(&self, cell: usize) -> Option<Shot> {
        self.shots.get(cell).copied().flatten()
    }

    /// How many cells have been shot.
    pub fn count(&self) -> usize {
        self.shots.iter().flatten().count()
    }

    /// Whether every cell of ship `index` of `fleet` has been hit.
    pub fn is_sunk(&self, fleet: &Fleet, index: usize) -> bool {
        fleet.cells_of(index).iter().all(|&c| self.shots[c] == Some(Shot::Hit))
    }

    /// Ships still afloat, as indexes into [`SHIPS`].
    pub fn afloat(&self, fleet: &Fleet) -> Vec<usize> {
        (0..SHIPS.len()).filter(|&i| !self.is_sunk(fleet, i)).collect()
    }

    pub fn all_sunk(&self, fleet: &Fleet) -> bool {
        self.afloat(fleet).is_empty()
    }

    /// Fires at `cell` of a sea holding `fleet`.
    pub fn fire(&mut self, fleet: &Fleet, cell: usize) -> Fired {
        if cell >= CELLS || self.shots[cell].is_some() {
            return Fired::Again;
        }
        match fleet.grid()[cell] {
            None => {
                self.shots[cell] = Some(Shot::Miss);
                Fired::Miss
            }
            Some(ship) => {
                self.shots[cell] = Some(Shot::Hit);
                if self.is_sunk(fleet, usize::from(ship)) {
                    Fired::Sunk(usize::from(ship))
                } else {
                    Fired::Hit
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_is_a_valid_fleet_and_they_differ() {
        let mut seen = Vec::new();
        for (name, code) in PRESETS {
            let fleet = Fleet::decode(code).unwrap_or_else(|| panic!("{name} is not a valid fleet"));
            assert_eq!(fleet.encode(), code);
            assert!(!seen.contains(&code), "{name} repeats a layout");
            seen.push(code);
            let covered = fleet.grid().iter().flatten().count();
            assert_eq!(covered, SHIPS.iter().map(|s| s.1).sum::<usize>(), "{name}");
        }
    }

    #[test]
    fn random_layouts_are_valid_and_follow_the_seed() {
        for seed in 5..400 {
            let fleet = Fleet::from_seed(seed);
            assert!(fleet.is_valid(), "seed {seed}");
            assert_eq!(fleet, Fleet::from_seed(seed), "seed {seed} is repeatable");
        }
        assert_ne!(Fleet::from_seed(1000), Fleet::from_seed(1001));
        assert_eq!(Fleet::from_seed(0).encode(), PRESETS[0].1);
    }

    #[test]
    fn broken_fleets_are_refused() {
        assert!(Fleet::decode("short").is_none());
        // The galleon runs off the right edge.
        assert!(Fleet::decode("60h09h92v96v45h").is_none());
        // The frigate lies on the galleon.
        assert!(Fleet::decode("00h01h92v96v45h").is_some(), "side by side is fine");
        assert!(Fleet::decode("00h02h92v96v45h").is_some());
        assert!(Fleet::decode("00h00h92v96v45h").is_none(), "two ships on one cell");
        assert!(Fleet::decode("00x09h92v96v45h").is_none());
        assert!(Fleet::decode("0a0h09h92v96v45").is_none());
    }

    #[test]
    fn cells_are_named_like_a_spreadsheet() {
        assert_eq!(cell_name(0), "A1");
        assert_eq!(cell_name(99), "J10");
        assert_eq!(cell_name(23), "D3");
        for cell in 0..CELLS {
            assert_eq!(parse_cell(&cell_name(cell)), Some(cell));
        }
        assert_eq!(parse_cell(" b7 "), Some(61));
        for bad in ["", "A", "A0", "A11", "K1", "1A", "AA1", "-"] {
            assert_eq!(parse_cell(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn shots_hit_miss_sink_and_are_not_repeated() {
        let fleet = Fleet::decode(PRESETS[0].1).expect("a preset");
        let mut sea = Sea::new();
        // The dinghy lies on E6 and F6.
        let dinghy = fleet.cells_of(4);
        assert_eq!(dinghy, [54, 55]);
        assert_eq!(sea.fire(&fleet, 0), Fired::Hit, "A1 is the galleon's bow");
        assert_eq!(sea.fire(&fleet, 0), Fired::Again);
        assert_eq!(sea.fire(&fleet, 99), Fired::Miss);
        assert_eq!(sea.fire(&fleet, 100), Fired::Again, "off the map");
        assert_eq!(sea.fire(&fleet, dinghy[0]), Fired::Hit);
        assert_eq!(sea.fire(&fleet, dinghy[1]), Fired::Sunk(4));
        assert!(sea.is_sunk(&fleet, 4) && !sea.is_sunk(&fleet, 0));
        assert_eq!(sea.afloat(&fleet), [0, 1, 2, 3]);
        assert_eq!(sea.count(), 4);
    }

    #[test]
    fn a_fleet_is_beaten_when_every_ship_is_sunk() {
        let fleet = Fleet::from_seed(7);
        let mut sea = Sea::new();
        let mut last = Fired::Miss;
        for cell in 0..CELLS {
            last = sea.fire(&fleet, cell);
            if sea.all_sunk(&fleet) {
                break;
            }
        }
        assert!(sea.all_sunk(&fleet));
        assert!(matches!(last, Fired::Sunk(_)), "the last shot sank the last ship");
        assert!(sea.afloat(&fleet).is_empty());
    }

    #[test]
    fn the_sea_round_trips_as_text() {
        let fleet = Fleet::from_seed(3);
        let mut sea = Sea::new();
        for cell in [0, 11, 22, 33, 44, 55, 66, 77, 88, 99] {
            sea.fire(&fleet, cell);
        }
        let text = sea.encode();
        assert_eq!(text.len(), CELLS);
        assert_eq!(Sea::decode(&text), Some(sea));
        assert!(Sea::decode("short").is_none());
        assert!(Sea::decode(&"x".repeat(CELLS)).is_none());
    }
}
