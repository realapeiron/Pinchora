//! Pixel Picross: 10×10 nonograms of sea objects. The clues are the lengths of the runs of filled cells
//! in each row and column; the picture is found when every line matches its clue.
//!
//! Columns read A to J and rows 1 to 10, like the coordinates in Sea Fleet.

/// Cells on a side.
pub const SIZE: usize = 10;
pub const CELLS: usize = SIZE * SIZE;

/// The day puzzle #1 was shown (days since 1970-01-01).
pub const LAUNCH_DAY: i64 = 20_762;

/// A picture: its name, its colour and which cells are filled (`#`).
pub struct Picture {
    pub name: &'static str,
    pub color: u32,
    pub rows: [&'static str; SIZE],
}

/// The pictures every puzzle is made from; a test checks that each one has exactly one solution.
pub const PICTURES: [Picture; 26] = [
    Picture {
        name: "Anchor",
        color: 0x8fa0d8,
        rows: [
            "....##....",
            "...#..#...",
            "....##....",
            "..######..",
            "....##....",
            "....##....",
            "#...##...#",
            "##..##..##",
            ".##.##.##.",
            "..######..",
        ],
    },
    Picture {
        name: "Fish",
        color: 0xff9e7a,
        rows: [
            "..........",
            "....###...",
            "#..#####..",
            "##.#######",
            "##.####.##",
            "##.#######",
            "#..#####..",
            "....###...",
            "..........",
            "..........",
        ],
    },
    Picture {
        name: "Shell",
        color: 0xffc4b0,
        rows: [
            "....##....",
            "..######..",
            ".########.",
            "#.#.##.#.#",
            "#.#.##.#.#",
            "##########",
            ".########.",
            "..######..",
            "...####...",
            "....##....",
        ],
    },
    Picture {
        name: "Crab",
        color: 0xff6b4a,
        rows: [
            "#........#",
            "##......##",
            "#.#.##.#.#",
            ".#######..",
            "..#.##.#..",
            ".########.",
            "##########",
            ".########.",
            "#.#.##.#.#",
            "..#....#..",
        ],
    },
    Picture {
        name: "Starfish",
        color: 0xffd166,
        rows: [
            "....##....",
            "....##....",
            "...####...",
            "##########",
            ".########.",
            "..######..",
            "..######..",
            ".###..###.",
            ".##....##.",
            "#........#",
        ],
    },
    Picture {
        name: "Octopus",
        color: 0x9b6bc4,
        rows: [
            "...####...",
            "..######..",
            ".########.",
            ".##.##.##.",
            ".########.",
            "..######..",
            ".#.#..#.#.",
            "#.#.##.#.#",
            "#.#.##.#.#",
            ".#..##..#.",
        ],
    },
    Picture {
        name: "Whale",
        color: 0x5bc0eb,
        rows: [
            "..........",
            "....#.#...",
            "....#.#...",
            "..########",
            ".#########",
            "##########",
            "#.########",
            "#########.",
            ".#######..",
            "..........",
        ],
    },
    Picture {
        name: "Sailboat",
        color: 0xfff1e8,
        rows: [
            "....#.....",
            "...##.....",
            "..###.....",
            ".####.....",
            "#####.##..",
            "#####.###.",
            "#####.####",
            "#####.....",
            "##########",
            ".########.",
        ],
    },
    Picture {
        name: "Lighthouse",
        color: 0xff8fb1,
        rows: [
            "....##....",
            "...####...",
            "..######..",
            "....##....",
            "...####...",
            "...#..#...",
            "...####...",
            "..#....#..",
            "..######..",
            ".########.",
        ],
    },
    Picture {
        name: "Seahorse",
        color: 0x06d6a0,
        rows: [
            "...###....",
            "..#.###...",
            "..#####...",
            "...##.....",
            "...###....",
            "...###....",
            "...###....",
            "..###.....",
            "..###.....",
            "...####...",
        ],
    },
    Picture {
        name: "Jellyfish",
        color: 0xcfb2ea,
        rows: [
            "..######..",
            ".########.",
            "##########",
            "##########",
            "##########",
            ".#.#.#.#.#",
            ".#.#.#.#.#",
            "..#.#.#.#.",
            ".#.#.#.#..",
            "..#.#.#.#.",
        ],
    },
    Picture {
        name: "Turtle",
        color: 0x5bbf6a,
        rows: [
            "..........",
            "...####...",
            "..######..",
            ".########.",
            "#.######.#",
            "..######..",
            ".########.",
            "##.####.##",
            "#........#",
            "..........",
        ],
    },
    Picture {
        name: "Oyster",
        color: 0xd6c8f0,
        rows: [
            "..........",
            "..######..",
            ".########.",
            "##########",
            "##########",
            ".###..###.",
            ".##.##.##.",
            "##.####.##",
            ".########.",
            "..######..",
        ],
    },
    Picture {
        name: "Chest",
        color: 0xc98a1e,
        rows: [
            "..........",
            ".########.",
            "##########",
            "#........#",
            "##########",
            "#...##...#",
            "#...##...#",
            "#........#",
            "##########",
            "..........",
        ],
    },
    Picture {
        name: "Trident",
        color: 0xffd166,
        rows: [
            "#...##...#",
            "#...##...#",
            "##..##..##",
            ".#..##..#.",
            ".########.",
            "..######..",
            "....##....",
            "....##....",
            "....##....",
            "....##....",
        ],
    },
    Picture {
        name: "Palm tree",
        color: 0x5bbf6a,
        rows: [
            "...##.##..",
            ".########.",
            "##########",
            "#.##..##.#",
            "..#.##.#..",
            ".....##...",
            ".....##...",
            ".....##...",
            "....###...",
            "..########",
        ],
    },
    Picture {
        name: "Dolphin",
        color: 0x3a9bdc,
        rows: [
            "..........",
            ".....##...",
            "....####..",
            "##.######.",
            "#########.",
            ".#########",
            "..#######.",
            "...####...",
            "..##..##..",
            "..........",
        ],
    },
    Picture {
        name: "Shark",
        color: 0x6a6f86,
        rows: [
            "....#.....",
            "....##....",
            "....###...",
            "...#####..",
            "..#######.",
            ".#########",
            "##########",
            "#.########",
            "..#.#.#.#.",
            "..........",
        ],
    },
    Picture {
        name: "Coral",
        color: 0xff6b4a,
        rows: [
            "#...#..#.#",
            "#...#..#.#",
            "##.##..###",
            ".####.##..",
            "..###.##..",
            "..###.##..",
            "...####...",
            "...####...",
            "..######..",
            ".########.",
        ],
    },
    Picture {
        name: "Pufferfish",
        color: 0xe8d3a3,
        rows: [
            "..........",
            "..#.#.#...",
            ".########.",
            "#.######.#",
            "##########",
            "#.#.##.#.#",
            "##########",
            "#.######..",
            ".########.",
            "..#.#.#...",
        ],
    },
    Picture {
        name: "Bottle",
        color: 0x8fd3ff,
        rows: [
            "....##....",
            "....##....",
            "....##....",
            "...####...",
            "..######..",
            "..#.##.#..",
            "..#.##.#..",
            "..#....#..",
            "..######..",
            "..######..",
        ],
    },
    Picture {
        name: "Sandcastle",
        color: 0xd2b77f,
        rows: [
            "#.#.##.#.#",
            "##########",
            "##########",
            ".########.",
            "..#.##.#..",
            "..#.##.#..",
            "..######..",
            "..#.##.#..",
            "##########",
            "##########",
        ],
    },
    Picture {
        name: "Seagull",
        color: 0xfff1e8,
        rows: [
            "..........",
            "..........",
            "##......##",
            ".###..###.",
            "..######..",
            "....##....",
            "....##....",
            "...####...",
            "...#..#...",
            "..........",
        ],
    },
    Picture {
        name: "Lobster",
        color: 0xe86a92,
        rows: [
            "##......##",
            "#.#....#.#",
            "#.#.##.#.#",
            "##.####.##",
            "...####...",
            "...####...",
            "..######..",
            "...####...",
            "..######..",
            ".###..###.",
        ],
    },
    Picture {
        name: "Lantern",
        color: 0xffd166,
        rows: [
            "....##....",
            "...####...",
            "..######..",
            "..#....#..",
            "..#.##.#..",
            "..#.##.#..",
            "..#.##.#..",
            "..#....#..",
            "..######..",
            "...####...",
        ],
    },
    Picture {
        name: "Key",
        color: 0xffd166,
        rows: [
            "..####....",
            ".##..##...",
            ".#....#...",
            ".##..##...",
            "..####....",
            "...##.....",
            "...##.....",
            "...###....",
            "...##.....",
            "...####...",
        ],
    },
];

/// A puzzle: a picture, mirrored sideways and/or flipped upside down (`variant` bit 0 and bit 1), so
/// each picture makes four different sets of clues.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Puzzle {
    pub picture: usize,
    pub variant: u8,
    cells: [bool; CELLS],
}

/// How a filled cell of the finished picture is shaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shade {
    /// Nothing above it: catches the light.
    Light,
    Main,
    /// Nothing below it, but something above: in shadow.
    Dark,
}

/// Which puzzle a day brings: every picture once in a scrambled order, then the same pictures
/// mirrored and flipped.
pub fn daily(day: i64) -> (usize, u8) {
    let n = PICTURES.len() as i64;
    // 7 and the number of pictures share no factor, so the order visits every picture.
    let picture = (day.rem_euclid(n) * 7 % n) as usize;
    let variant = (day.div_euclid(n).rem_euclid(4)) as u8;
    (picture, variant)
}

/// The number shown on a day's puzzle.
pub fn puzzle_number(day: i64) -> i64 {
    day - LAUNCH_DAY + 1
}

impl Puzzle {
    pub fn new(picture: usize, variant: u8) -> Self {
        let picture = picture % PICTURES.len();
        let variant = variant % 4;
        let rows = &PICTURES[picture].rows;
        let mut cells = [false; CELLS];
        for (y, row) in rows.iter().enumerate() {
            for (x, c) in row.chars().enumerate() {
                let (tx, ty) = (
                    if variant & 1 == 1 { SIZE - 1 - x } else { x },
                    if variant & 2 == 2 { SIZE - 1 - y } else { y },
                );
                cells[ty * SIZE + tx] = c == '#';
            }
        }
        Self {
            picture,
            variant,
            cells,
        }
    }

    pub fn name(&self) -> &'static str {
        PICTURES[self.picture].name
    }

    pub fn color(&self) -> u32 {
        PICTURES[self.picture].color
    }

    /// Whether `cell` is filled in the finished picture.
    pub fn filled(&self, cell: usize) -> bool {
        self.cells[cell]
    }

    pub fn solution(&self) -> &[bool; CELLS] {
        &self.cells
    }

    /// How a filled cell of the finished picture is shaded.
    pub fn shade(&self, cell: usize) -> Shade {
        let above = cell >= SIZE && self.cells[cell - SIZE];
        let below = cell + SIZE < CELLS && self.cells[cell + SIZE];
        if !above {
            Shade::Light
        } else if !below {
            Shade::Dark
        } else {
            Shade::Main
        }
    }

    /// The clues of every row, then of every column.
    pub fn clues(&self) -> Clues {
        clues_of(&self.cells)
    }
}

/// The runs of filled cells of each row and each column; a line with none has the clue `[0]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clues {
    pub rows: Vec<Vec<u8>>,
    pub cols: Vec<Vec<u8>>,
}

fn runs(line: impl Iterator<Item = bool>) -> Vec<u8> {
    let mut out = Vec::new();
    let mut run = 0u8;
    for filled in line {
        if filled {
            run += 1;
        } else if run > 0 {
            out.push(run);
            run = 0;
        }
    }
    if run > 0 {
        out.push(run);
    }
    if out.is_empty() {
        out.push(0);
    }
    out
}

pub fn clues_of(cells: &[bool; CELLS]) -> Clues {
    Clues {
        rows: (0..SIZE)
            .map(|y| runs((0..SIZE).map(|x| cells[y * SIZE + x])))
            .collect(),
        cols: (0..SIZE)
            .map(|x| runs((0..SIZE).map(|y| cells[y * SIZE + x])))
            .collect(),
    }
}

/// A player's marks: which cells they have filled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grid {
    cells: [bool; CELLS],
}

impl Default for Grid {
    fn default() -> Self {
        Self::new()
    }
}

impl Grid {
    pub fn new() -> Self {
        Self { cells: [false; CELLS] }
    }

    /// 100 characters, `#` filled and `.` empty.
    pub fn encode(&self) -> String {
        self.cells.iter().map(|&c| if c { '#' } else { '.' }).collect()
    }

    pub fn decode(text: &str) -> Option<Self> {
        if text.len() != CELLS {
            return None;
        }
        let mut cells = [false; CELLS];
        for (cell, c) in cells.iter_mut().zip(text.chars()) {
            *cell = match c {
                '#' => true,
                '.' => false,
                _ => return None,
            };
        }
        Some(Self { cells })
    }

    pub fn get(&self, cell: usize) -> bool {
        self.cells[cell]
    }

    pub fn set(&mut self, cell: usize, filled: bool) {
        self.cells[cell] = filled;
    }

    pub fn cells(&self) -> &[bool; CELLS] {
        &self.cells
    }

    /// How many cells are filled.
    pub fn count(&self) -> usize {
        self.cells.iter().filter(|&&c| c).count()
    }

    /// Whether every row and column matches its clue.
    pub fn solves(&self, clues: &Clues) -> bool {
        clues_of(&self.cells) == *clues
    }

    /// Filled cells that the finished picture leaves empty.
    pub fn mistakes(&self, puzzle: &Puzzle) -> Vec<usize> {
        (0..CELLS).filter(|&c| self.cells[c] && !puzzle.filled(c)).collect()
    }

    /// Whether row `y` is a match for its clue (shown as a tick beside it).
    pub fn row_done(&self, clues: &Clues, y: usize) -> bool {
        runs((0..SIZE).map(|x| self.cells[y * SIZE + x])) == clues.rows[y]
    }

    pub fn col_done(&self, clues: &Clues, x: usize) -> bool {
        runs((0..SIZE).map(|y| self.cells[y * SIZE + x])) == clues.cols[x]
    }
}

/// What a typed list of cells asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marks {
    pub fill: Vec<usize>,
    pub clear: Vec<usize>,
}

/// Reads `A1 B3-B7 -C2 -D1-D4`: cells and straight runs (`B3-B7` along a column, `A1-E1` along a row)
/// to fill, or to clear when they start with `-`. Words are split by spaces or commas. `None` for
/// anything it cannot read, such as a diagonal.
pub fn parse_marks(text: &str) -> Option<Marks> {
    let mut marks = Marks {
        fill: Vec::new(),
        clear: Vec::new(),
    };
    let mut any = false;
    for word in text
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|w| !w.is_empty())
    {
        let (clearing, word) = match word.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, word),
        };
        let (first, last) = match word.split_once('-') {
            Some((a, b)) => (parse_cell(a)?, parse_cell(b)?),
            None => (parse_cell(word)?, parse_cell(word)?),
        };
        let (x0, y0, x1, y1) = (first % SIZE, first / SIZE, last % SIZE, last / SIZE);
        if x0 != x1 && y0 != y1 {
            return None;
        }
        let cells: Vec<usize> = (y0.min(y1)..=y0.max(y1))
            .flat_map(|y| (x0.min(x1)..=x0.max(x1)).map(move |x| y * SIZE + x))
            .collect();
        if clearing {
            marks.clear.extend(cells);
        } else {
            marks.fill.extend(cells);
        }
        any = true;
    }
    any.then_some(marks)
}

fn parse_cell(text: &str) -> Option<usize> {
    let mut chars = text.chars();
    let column = chars.next()?.to_ascii_uppercase();
    let row: usize = chars.as_str().parse().ok()?;
    let x = (column as usize).checked_sub('A' as usize).filter(|&x| x < SIZE)?;
    (1..=SIZE).contains(&row).then(|| (row - 1) * SIZE + x)
}

/// Every way to lay `clue` along a line of `SIZE` cells, as bit masks (bit `i` is cell `i`).
fn placements(clue: &[u8]) -> Vec<u16> {
    fn go(clue: &[u8], from: usize, mask: u16, out: &mut Vec<u16>) {
        let Some((&len, rest)) = clue.split_first() else {
            out.push(mask);
            return;
        };
        let len = usize::from(len);
        let needed: usize = rest.iter().map(|&r| usize::from(r) + 1).sum();
        let mut start = from;
        while start + len + needed <= SIZE {
            let run = ((1u16 << len) - 1) << start;
            go(rest, start + len + 1, mask | run, out);
            start += 1;
        }
    }
    let mut out = Vec::new();
    if clue == [0] {
        out.push(0);
    } else {
        go(clue, 0, 0, &mut out);
    }
    out
}

/// How many pictures fit these clues, counting no further than `limit`. One means the clues have a single
/// solution, which is what every puzzle should have.
pub fn count_solutions(clues: &Clues, limit: usize) -> usize {
    let rows: Vec<Vec<u16>> = clues.rows.iter().map(|c| placements(c)).collect();
    let cols: Vec<Vec<u16>> = clues.cols.iter().map(|c| placements(c)).collect();
    // For each row, the cells known filled and the cells known empty (bit x is column x).
    let state = ([0u16; SIZE], [0u16; SIZE]);
    let mut found = 0;
    search(&rows, &cols, state, limit, &mut found);
    found
}

type Known = ([u16; SIZE], [u16; SIZE]);

const FULL: u16 = (1 << SIZE) - 1;

/// Narrows what is known by the rows and columns until nothing changes; false on a contradiction.
#[allow(clippy::needless_range_loop)] // `y` and `x` index the rows' and the columns' bit masks at once
fn propagate(rows: &[Vec<u16>], cols: &[Vec<u16>], known: &mut Known) -> bool {
    loop {
        let before = *known;
        for y in 0..SIZE {
            let (filled, empty) = (known.0[y], known.1[y]);
            let mut all = FULL;
            let mut any = 0u16;
            let mut fits = false;
            for &p in &rows[y] {
                if p & empty == 0 && p & filled == filled {
                    all &= p;
                    any |= p;
                    fits = true;
                }
            }
            if !fits {
                return false;
            }
            known.0[y] |= all;
            known.1[y] |= !any & FULL;
        }
        for x in 0..SIZE {
            let column = |rows: &[u16; SIZE]| (0..SIZE).fold(0u16, |m, y| m | (((rows[y] >> x) & 1) << y));
            let (filled, empty) = (column(&known.0), column(&known.1));
            let mut all = FULL;
            let mut any = 0u16;
            let mut fits = false;
            for &p in &cols[x] {
                if p & empty == 0 && p & filled == filled {
                    all &= p;
                    any |= p;
                    fits = true;
                }
            }
            if !fits {
                return false;
            }
            for y in 0..SIZE {
                known.0[y] |= ((all >> y) & 1) << x;
                known.1[y] |= ((!any & FULL) >> y & 1) << x;
            }
        }
        if *known == before {
            return true;
        }
    }
}

fn search(rows: &[Vec<u16>], cols: &[Vec<u16>], mut known: Known, limit: usize, found: &mut usize) {
    if *found >= limit || !propagate(rows, cols, &mut known) {
        return;
    }
    let open = (0..SIZE).find_map(|y| {
        let unknown = !(known.0[y] | known.1[y]) & FULL;
        (unknown != 0).then(|| (y, unknown.trailing_zeros() as usize))
    });
    let Some((y, x)) = open else {
        *found += 1;
        return;
    };
    for filled in [true, false] {
        let mut next = known;
        if filled {
            next.0[y] |= 1 << x;
        } else {
            next.1[y] |= 1 << x;
        }
        search(rows, cols, next, limit, found);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_picture_has_exactly_one_solution_in_every_variant() {
        for (index, picture) in PICTURES.iter().enumerate() {
            for (y, row) in picture.rows.iter().enumerate() {
                assert_eq!(row.len(), SIZE, "{} row {y}", picture.name);
                assert!(row.chars().all(|c| c == '#' || c == '.'), "{} row {y}", picture.name);
            }
            for variant in 0..4 {
                let puzzle = Puzzle::new(index, variant);
                assert_eq!(
                    count_solutions(&puzzle.clues(), 2),
                    1,
                    "{} (variant {variant}) must have one solution",
                    picture.name
                );
            }
        }
    }

    #[test]
    fn pictures_are_not_empty_and_have_their_own_names_and_colours() {
        let names: std::collections::HashSet<_> = PICTURES.iter().map(|p| p.name).collect();
        assert_eq!(names.len(), PICTURES.len());
        for picture in &PICTURES {
            let filled = picture
                .rows
                .iter()
                .flat_map(|r| r.chars())
                .filter(|&c| c == '#')
                .count();
            assert!((24..=85).contains(&filled), "{} fills {filled} cells", picture.name);
        }
    }

    #[test]
    fn clues_are_the_runs_of_each_line() {
        assert_eq!(runs([true, true, false, true].into_iter()), [2, 1]);
        assert_eq!(runs([false, false].into_iter()), [0]);
        assert_eq!(runs([true; 10].into_iter()), [10]);
        let puzzle = Puzzle::new(0, 0);
        let clues = puzzle.clues();
        // The anchor's top row is the ring: two filled cells.
        assert_eq!(clues.rows[0], [2]);
        assert_eq!(clues.rows[3], [6]);
        assert_eq!(clues.rows.len() + clues.cols.len(), 20);
    }

    #[test]
    fn variants_mirror_and_flip() {
        let base = Puzzle::new(1, 0);
        let mirrored = Puzzle::new(1, 1);
        let flipped = Puzzle::new(1, 2);
        for y in 0..SIZE {
            for x in 0..SIZE {
                assert_eq!(mirrored.filled(y * SIZE + x), base.filled(y * SIZE + SIZE - 1 - x));
                assert_eq!(flipped.filled(y * SIZE + x), base.filled((SIZE - 1 - y) * SIZE + x));
            }
        }
        assert_eq!(Puzzle::new(1, 4), base, "variants wrap");
        assert_eq!(Puzzle::new(27, 0), Puzzle::new(1, 0), "so do pictures");
    }

    #[test]
    fn the_daily_puzzle_visits_every_picture_then_comes_back_changed() {
        let n = PICTURES.len() as i64;
        let mut seen = std::collections::HashSet::new();
        for day in 20_000..20_000 + n {
            let (picture, variant) = daily(day);
            assert!(seen.insert(picture), "picture {picture} repeats on day {day}");
            assert!(variant < 4);
        }
        assert_eq!(daily(20_763), daily(20_763));
        assert_ne!(daily(20_763), daily(20_764));
        let (a, va) = daily(20_000);
        let (b, vb) = daily(20_000 + n);
        assert_eq!(a, b);
        assert_ne!(va, vb, "the second time round it is mirrored or flipped");
        assert_eq!(puzzle_number(LAUNCH_DAY), 1);
    }

    #[test]
    fn a_grid_solves_when_every_line_matches_and_any_matching_grid_counts() {
        let puzzle = Puzzle::new(3, 0);
        let clues = puzzle.clues();
        let mut grid = Grid::new();
        assert!(!grid.solves(&clues));
        for cell in 0..CELLS {
            grid.set(cell, puzzle.filled(cell));
        }
        assert!(grid.solves(&clues));
        assert!(grid.mistakes(&puzzle).is_empty());
        assert!((0..SIZE).all(|i| grid.row_done(&clues, i) && grid.col_done(&clues, i)));
        let first = (0..CELLS).find(|&c| puzzle.filled(c)).expect("a filled cell");
        grid.set(first, false);
        assert!(!grid.solves(&clues));
        assert!(!grid.row_done(&clues, first / SIZE));
        let empty = (0..CELLS).find(|&c| !puzzle.filled(c)).expect("an empty cell");
        grid.set(first, true);
        grid.set(empty, true);
        assert_eq!(grid.mistakes(&puzzle), [empty]);
        assert_eq!(grid.count(), puzzle.solution().iter().filter(|&&c| c).count() + 1);
    }

    #[test]
    fn grids_round_trip_as_text() {
        let mut grid = Grid::new();
        grid.set(0, true);
        grid.set(99, true);
        let text = grid.encode();
        assert_eq!(text.len(), CELLS);
        assert_eq!(Grid::decode(&text), Some(grid));
        assert!(Grid::decode("short").is_none());
        assert!(Grid::decode(&"x".repeat(CELLS)).is_none());
    }

    #[test]
    fn typed_cells_and_runs_are_read() {
        let marks = parse_marks("A1 b3-b5, -C2 -d1-d2").expect("readable");
        assert_eq!(marks.fill, [0, 21, 31, 41]);
        assert_eq!(marks.clear, [12, 3, 13]);
        let row = parse_marks("a2-e2").expect("a run along a row");
        assert_eq!(row.fill, [10, 11, 12, 13, 14]);
        let backwards = parse_marks("e2-a2").expect("either way round");
        assert_eq!(backwards.fill, row.fill);
        for bad in ["", "   ", "A11", "K1", "A1-B2", "A0", "1A", "A1-", "--A1", "A1 ??"] {
            assert!(parse_marks(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn the_solver_counts_several_solutions_when_the_clues_allow_them() {
        // A 2x2 block of ones has two solutions (a diagonal or the other); embed it in empty lines.
        let clues = Clues {
            rows: [vec![1], vec![1]].into_iter().chain((0..8).map(|_| vec![0])).collect(),
            cols: [vec![1], vec![1]].into_iter().chain((0..8).map(|_| vec![0])).collect(),
        };
        assert_eq!(count_solutions(&clues, 5), 2);
        assert_eq!(count_solutions(&clues, 1), 1, "it stops at the limit");
        // Clues that no picture fits.
        let impossible = Clues {
            rows: (0..SIZE).map(|_| vec![10]).collect(),
            cols: (0..SIZE).map(|_| vec![0]).collect(),
        };
        assert_eq!(count_solutions(&impossible, 2), 0);
    }

    #[test]
    fn shading_lights_the_top_and_shades_the_bottom() {
        let puzzle = Puzzle::new(0, 0);
        let top = puzzle.clues().rows.iter().position(|r| r != &[0]).unwrap();
        let first = (0..SIZE).map(|x| top * SIZE + x).find(|&c| puzzle.filled(c)).unwrap();
        assert_eq!(puzzle.shade(first), Shade::Light);
        let bottom_cell = (0..CELLS).rev().find(|&c| puzzle.filled(c)).unwrap();
        assert_ne!(puzzle.shade(bottom_cell), Shade::Light);
    }
}
