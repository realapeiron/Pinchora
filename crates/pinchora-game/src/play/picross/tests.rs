use super::*;

#[test]
fn every_picture_has_exactly_one_solution_in_every_variant() {
    for (index, picture) in PICTURES.iter().enumerate() {
        for (y, row) in picture.rows.iter().enumerate() {
            assert_eq!(row.len(), SIZE, "{} row {y}", picture.name);
            assert!(row.chars().all(|c| ".#aew".contains(c)), "{} row {y}", picture.name);
        }
        for variant in 0..VARIANTS {
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

/// The number of pieces a picture falls into, cells touching at a corner counting as joined.
fn pieces(puzzle: &Puzzle) -> usize {
    let mut seen = [false; CELLS];
    let mut count = 0;
    for start in (0..CELLS).filter(|&c| puzzle.filled(c)) {
        if seen[start] {
            continue;
        }
        count += 1;
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(cell) = stack.pop() {
            let (x, y) = ((cell % SIZE) as i32, (cell / SIZE) as i32);
            for (dx, dy) in (-1..=1).flat_map(|dx| (-1..=1).map(move |dy| (dx, dy))) {
                let (nx, ny) = (x + dx, y + dy);
                if !(0..SIZE as i32).contains(&nx) || !(0..SIZE as i32).contains(&ny) {
                    continue;
                }
                let next = ny as usize * SIZE + nx as usize;
                if puzzle.filled(next) && !seen[next] {
                    seen[next] = true;
                    stack.push(next);
                }
            }
        }
    }
    count
}

#[test]
fn pictures_are_in_one_piece_apart_from_what_sits_inside_them() {
    for (index, picture) in PICTURES.iter().enumerate() {
        let pieces = pieces(&Puzzle::new(index, 0));
        // The flame in the lantern floats in its glass; nothing else is cut loose, and the fish's
        // tail is joined to its body.
        let allowed = if picture.name == "Lantern" { 2 } else { 1 };
        assert_eq!(pieces, allowed, "{} falls into {pieces} pieces", picture.name);
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
            .filter(|&c| c != '.')
            .count();
        assert!((24..=85).contains(&filled), "{} fills {filled} cells", picture.name);
    }
}

#[test]
fn every_picture_has_a_name_in_every_language() {
    for (index, picture) in PICTURES.iter().enumerate() {
        let puzzle = Puzzle::new(index, 0);
        assert_eq!(
            puzzle.name_in(Lang::En),
            picture.name,
            "English reads as the picture's own name"
        );
        let turkish = puzzle.name_in(Lang::Tr);
        assert!(
            !turkish.starts_with("picross.name."),
            "{} has no Turkish name",
            picture.name
        );
    }
}

#[test]
fn no_two_pictures_are_the_same_shape_even_mirrored() {
    let shape = |index: usize, variant: u8| Puzzle::new(index, variant).solution().to_vec();
    for (a, first) in PICTURES.iter().enumerate() {
        for (b, second) in PICTURES.iter().enumerate().skip(a + 1) {
            for variant in 0..VARIANTS {
                assert_ne!(
                    shape(a, 0),
                    shape(b, variant),
                    "{} and {} are the same shape",
                    first.name,
                    second.name
                );
            }
        }
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
fn variants_mirror_sideways_and_never_flip() {
    let base = Puzzle::new(1, 0);
    let mirrored = Puzzle::new(1, 1);
    for y in 0..SIZE {
        for x in 0..SIZE {
            assert_eq!(mirrored.filled(y * SIZE + x), base.filled(y * SIZE + SIZE - 1 - x));
            assert_eq!(mirrored.ink(y * SIZE + x), base.ink(y * SIZE + SIZE - 1 - x));
        }
    }
    assert_eq!(VARIANTS, 2, "no upside-down pictures");
    assert_eq!(Puzzle::new(1, 2), base, "variants wrap");
    assert_eq!(Puzzle::new(PICTURES.len() + 1, 0), Puzzle::new(1, 0), "so do pictures");
}

#[test]
fn the_fishs_tail_is_joined_and_its_eye_and_belly_are_painted() {
    let fish = PICTURES.iter().position(|p| p.name == "Fish").expect("a fish");
    let puzzle = Puzzle::new(fish, 0);
    // Row 3 runs unbroken from the tail's tip to the nose.
    assert!((0..SIZE).all(|x| puzzle.filled(3 * SIZE + x)), "the tail is attached");
    let inks: Vec<Ink> = (0..CELLS).filter_map(|c| puzzle.ink(c)).collect();
    for ink in [Ink::Body, Ink::Accent, Ink::White, Ink::Eye] {
        assert!(inks.contains(&ink), "{ink:?}");
    }
}

#[test]
fn the_daily_puzzle_visits_every_picture_then_comes_back_mirrored() {
    let n = PICTURES.len() as i64;
    let mut seen = std::collections::HashSet::new();
    for day in 20_000..20_000 + n {
        let (picture, variant) = daily(day);
        assert!(seen.insert(picture), "picture {picture} repeats on day {day}");
        assert!(variant < VARIANTS);
    }
    assert_eq!(daily(20_763), daily(20_763));
    assert_ne!(daily(20_763), daily(20_764));
    let (a, va) = daily(20_000);
    let (b, vb) = daily(20_000 + n);
    assert_eq!(a, b);
    assert_ne!(va, vb, "the second time round it is mirrored");
    assert_eq!(puzzle_number(LAUNCH_DAY), 1);
}

#[test]
fn practice_never_uses_the_pictures_of_the_coming_dailies() {
    for day in 20_000..20_000 + 3 * PICTURES.len() as i64 {
        let allowed = practice_pictures(day);
        assert_eq!(
            allowed.len(),
            PICTURES.len() - PRACTICE_BLACKOUT_DAYS as usize,
            "day {day}"
        );
        for ahead in 0..PRACTICE_BLACKOUT_DAYS {
            let (picture, _) = daily(day + ahead);
            assert!(
                !allowed.contains(&picture),
                "day {day}: picture {picture} is the daily in {ahead} days"
            );
        }
        assert!(allowed.iter().all(|&p| p < PICTURES.len()));
    }
    // Pictures come back into practice once their daily is more than a week away or past.
    let (picture, _) = daily(20_100);
    assert!(!practice_pictures(20_100).contains(&picture));
    assert!(
        practice_pictures(20_101).contains(&picture),
        "the day after, it is free again"
    );
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
    assert_eq!(grid.wrong(&puzzle), 0);
    assert!((0..SIZE).all(|i| grid.row_done(&clues, i) && grid.col_done(&clues, i)));
    let first = (0..CELLS).find(|&c| puzzle.filled(c)).expect("a filled cell");
    grid.set(first, false);
    assert!(!grid.solves(&clues));
    assert!(!grid.row_done(&clues, first / SIZE));
    let empty = (0..CELLS).find(|&c| !puzzle.filled(c)).expect("an empty cell");
    grid.set(first, true);
    grid.set(empty, true);
    assert_eq!(grid.wrong(&puzzle), 1);
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
