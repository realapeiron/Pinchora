//! Sea Fleet, battleship on a 10×10 sea: fleets, random layouts, and shots.
//!
//! A cell is `y * 10 + x`; columns read A to J along the top and rows 1 to 10 down the side.

pub const SIZE: usize = 10;
pub const CELLS: usize = SIZE * SIZE;

/// The five ships of a fleet and how many cells each takes.
pub const SHIPS: [(&str, usize); 5] = [("Galleon", 5), ("Frigate", 4), ("Brig", 3), ("Sloop", 3), ("Dinghy", 2)];

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

/// The cells a ship of `len` cells covers where it lies.
fn cells(ship: Placed, len: usize) -> impl Iterator<Item = usize> {
    (0..len).map(move |k| {
        if ship.horizontal {
            ship.y * SIZE + ship.x + k
        } else {
            (ship.y + k) * SIZE + ship.x
        }
    })
}

impl Fleet {
    /// The cells ship `index` covers.
    pub fn cells_of(&self, index: usize) -> Vec<usize> {
        cells(self.ships[index], SHIPS[index].1).collect()
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

    /// A layout drawn uniformly from every legal one: each ship goes anywhere in the sea, either way round,
    /// and a draw where two ships meet is thrown away whole, so no layout is likelier than another. `below`
    /// gives a whole number in `0..bound`; the bot passes its secure random source, so a layout cannot be
    /// worked out from the ones before it or from the shots fired.
    pub fn random(below: &mut dyn FnMut(usize) -> usize) -> Self {
        loop {
            let mut taken = [false; CELLS];
            let mut ships = [Placed {
                x: 0,
                y: 0,
                horizontal: true,
            }; 5];
            let mut clear = true;
            for (index, (_, len)) in SHIPS.iter().enumerate() {
                // Every way to lay the ship: along the rows first, then down the columns.
                let across = (SIZE - len + 1) * SIZE;
                let pick = below(2 * across);
                let horizontal = pick < across;
                let at = pick % across;
                let ship = if horizontal {
                    Placed {
                        x: at % (SIZE - len + 1),
                        y: at / (SIZE - len + 1),
                        horizontal,
                    }
                } else {
                    Placed {
                        x: at % SIZE,
                        y: at / SIZE,
                        horizontal,
                    }
                };
                ships[index] = ship;
                for cell in cells(ship, *len) {
                    clear &= !std::mem::replace(&mut taken[cell], true);
                }
            }
            if clear {
                return Self { ships };
            }
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

    /// Five ships along the edges and the middle, written out.
    const HARBOUR: &str = "00h09h92v96v45h";

    /// A deterministic stream of numbers for tests (xorshift64*), not for the bot.
    fn stream(seed: u64) -> impl FnMut(usize) -> usize {
        let mut state = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
        move |bound| {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            (state.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 33) as usize % bound
        }
    }

    #[test]
    fn random_layouts_are_valid_and_come_from_the_whole_sea() {
        let mut below = stream(7);
        let mut seen = std::collections::HashSet::new();
        let mut covered = [false; CELLS];
        let mut horizontal = [0usize; 5];
        const DRAWS: usize = 4_000;
        for _ in 0..DRAWS {
            let fleet = Fleet::random(&mut below);
            assert!(fleet.is_valid());
            assert_eq!(fleet.grid().iter().flatten().count(), 17, "five ships, seventeen cells");
            assert_eq!(Fleet::decode(&fleet.encode()), Some(fleet.clone()), "it round-trips");
            for (ship, count) in horizontal.iter_mut().enumerate() {
                *count += usize::from(fleet.ships[ship].horizontal);
            }
            for (cell, ship) in fleet.grid().iter().enumerate() {
                covered[cell] |= ship.is_some();
            }
            seen.insert(fleet.encode());
        }
        assert!(
            seen.len() > DRAWS - 5,
            "layouts do not repeat: {} of {DRAWS}",
            seen.len()
        );
        assert!(
            covered.iter().all(|&c| c),
            "every cell of the sea gets a ship sometimes"
        );
        for (ship, count) in horizontal.into_iter().enumerate() {
            let share = count as f64 / DRAWS as f64;
            assert!(
                (0.45..0.55).contains(&share),
                "ship {ship} lies across {share:.2} of the time"
            );
        }
    }

    #[test]
    fn the_layouts_are_uniform_over_where_one_ship_can_lie() {
        // The dinghy (two cells) has 2 * 9 * 10 = 180 places; once the fleet is valid each is as likely
        // as any other, give or take what the other ships take up.
        let mut below = stream(11);
        let mut hits = std::collections::HashMap::new();
        const DRAWS: usize = 36_000;
        for _ in 0..DRAWS {
            let ship = Fleet::random(&mut below).ships[4];
            *hits.entry((ship.x, ship.y, ship.horizontal)).or_insert(0usize) += 1;
        }
        assert_eq!(hits.len(), 180, "every place is used");
        let (low, high) = hits.values().fold((usize::MAX, 0), |(l, h), &n| (l.min(n), h.max(n)));
        // About 200 each; the corners and edges are not special.
        assert!(low > 120 && high < 290, "{low} to {high}");
    }

    #[test]
    fn a_fresh_random_fleet_is_not_one_of_a_handful_of_known_ones() {
        let mut below = stream(3);
        let first: Vec<String> = (0..50).map(|_| Fleet::random(&mut below).encode()).collect();
        let distinct: std::collections::HashSet<&String> = first.iter().collect();
        assert_eq!(distinct.len(), 50);
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
        let fleet = Fleet::decode(HARBOUR).expect("a valid fleet");
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
        let fleet = Fleet::random(&mut stream(7));
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
        let fleet = Fleet::random(&mut stream(3));
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
