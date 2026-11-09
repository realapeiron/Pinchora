use super::*;

/// How many ways a picture is set: as drawn, or mirrored sideways. Never upside down, which would make a
/// sea creature hard to know.
pub const VARIANTS: u8 = 2;

/// A puzzle: a picture, mirrored sideways or not (`variant` 1 or 0), so each picture makes two different
/// sets of clues.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Puzzle {
    pub picture: usize,
    pub variant: u8,
    cells: [bool; CELLS],
    inks: [Option<Ink>; CELLS],
}

/// What a filled cell of the finished picture is painted in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ink {
    /// The picture's own colour.
    Body,
    /// Its second colour.
    Accent,
    White,
    /// The dark of an eye.
    Eye,
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
/// mirrored.
pub fn daily(day: i64) -> (usize, u8) {
    let n = PICTURES.len() as i64;
    // 7 and the number of pictures share no factor, so the order visits every picture.
    let picture = (day.rem_euclid(n) * 7 % n) as usize;
    let variant = (day.div_euclid(n).rem_euclid(i64::from(VARIANTS))) as u8;
    (picture, variant)
}

/// How many days, today included, the daily puzzle's pictures are kept out of practice.
pub const PRACTICE_BLACKOUT_DAYS: i64 = 7;

/// The pictures a practice puzzle may be made from on `day`: all but the ones the daily puzzle shows today
/// and in the next days, so nobody can solve the day's puzzle ahead of time in practice (in either variant).
pub fn practice_pictures(day: i64) -> Vec<usize> {
    let kept: Vec<usize> = (day..day + PRACTICE_BLACKOUT_DAYS).map(|d| daily(d).0).collect();
    (0..PICTURES.len()).filter(|p| !kept.contains(p)).collect()
}

/// The number shown on a day's puzzle.
pub fn puzzle_number(day: i64) -> i64 {
    day - LAUNCH_DAY + 1
}

impl Puzzle {
    pub fn new(picture: usize, variant: u8) -> Self {
        let picture = picture % PICTURES.len();
        let variant = variant % VARIANTS;
        let rows = &PICTURES[picture].rows;
        let mut cells = [false; CELLS];
        let mut inks = [None; CELLS];
        for (y, row) in rows.iter().enumerate() {
            for (x, c) in row.chars().enumerate() {
                let tx = if variant == 1 { SIZE - 1 - x } else { x };
                let ink = match c {
                    '#' => Some(Ink::Body),
                    'a' => Some(Ink::Accent),
                    'w' => Some(Ink::White),
                    'e' => Some(Ink::Eye),
                    _ => None,
                };
                cells[y * SIZE + tx] = ink.is_some();
                inks[y * SIZE + tx] = ink;
            }
        }
        Self {
            picture,
            variant,
            cells,
            inks,
        }
    }

    pub fn name(&self) -> &'static str {
        PICTURES[self.picture].name
    }

    /// The picture's name in `lang`: `picross.name.<key>` in the catalogs, where the key is the English name
    /// in lowercase with dashes (`palm-tree`).
    pub fn name_in(&self, lang: Lang) -> String {
        let key: String = PICTURES[self.picture]
            .name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        t!(lang, &format!("picross.name.{key}"))
    }

    pub fn color(&self) -> u32 {
        PICTURES[self.picture].color
    }

    pub fn accent(&self) -> u32 {
        PICTURES[self.picture].accent
    }

    /// What `cell` of the finished picture is painted in; `None` where it is empty.
    pub fn ink(&self, cell: usize) -> Option<Ink> {
        self.inks[cell]
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

pub(super) fn runs(line: impl Iterator<Item = bool>) -> Vec<u8> {
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

    /// How many filled cells the finished picture leaves empty.
    pub fn wrong(&self, puzzle: &Puzzle) -> usize {
        (0..CELLS).filter(|&c| self.cells[c] && !puzzle.filled(c)).count()
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

/// Reads `A1 B3-B7 -C2 -`: cells and straight runs (`B3-B7` along a column, `A1-E1` along a row)
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
