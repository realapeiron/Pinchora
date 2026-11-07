//! Every card draws whatever a player has typed into it: names, bios and titles holding emoji, CJK,
//! markup and the code points XML cannot hold (U+FFFE and U+FFFF, the other noncharacters, control
//! characters) must not stop a card from rendering.

use image::RgbaImage;
use pinchora_core::card_design::{CardKind, Content};
use pinchora_render::{
    AchievementCell, AchievementsCard, Backdrop, BadgeCell, BadgeCollectionCard, BadgeIcon, BadgeState, BottleCard,
    CardFill, CharacterCard, CheckersCard, ChessCard, ColorCard, ColorCell, Connect4Card, CrewView, DerbyCard,
    DerbyLane, DesignedCard, DexCard, DexCell, DropCard, DropCritter, EditorCell, EditorSection, EntriesCard,
    EntryHeader, EntryPicture, EntryRow, FleetCard, GridSlot, HangmanCard, HangmanLetter, HaulCard, HaulTile,
    HeistCard, HelpCard, HelpRow, ItemGridCard, LastWeekEntry, LeaderboardCard, LeaderboardEntry, LettersCard,
    LevelUpCard, Mood, NextRole, NurseryCard, NurseryPet, PartyLobbyCard, PearlCard, PearlSeat, PetCard, PetStatus,
    PickView, PicrossCard, PollCard, PollOption, PostcardCard, PotCard, PotState, ProfileCard, PromptCard, PromptTone,
    QuickCard, RaceCard, Racer, RacerState, RadarCard, RankCard, RatherCard, Renderer, Reveal, SeaView, ShareGrabCard,
    ShellCard, ShipView, ShowcaseKind, ShowcaseSlot, SpriteArt, StageView, StandingRow, StandingsCard, SurgeCard,
    SurgeState, Theme, Tide2048Card, WalletCard, WelcomeCard, WordleCard, WordleRow, WrappedCard, WrappedFish,
    WrappedHouse, WrappedTotals, encode_png, templates,
};

/// Emoji, a Chinese letter, markup, then code points XML 1.0 forbids or a parser may refuse: U+FFFE, U+FFFF, a
/// noncharacter of the last plane, U+FDD0, NUL, a bell, a vertical tab, DEL and a C1 control.
const HOSTILE: &str =
    "Zoë ✨ 李 😀 <b>&amp;\"'</b> \u{FFFE}\u{FFFF}\u{1FFFE}\u{10FFFF}\u{FDD0}\u{0}\u{7}\u{B}\u{7F}\u{85} end";
/// Just the forbidden ones, for markup written by hand.
const FORBIDDEN: &str = "\u{FFFE}\u{FFFF}\u{1FFFE}\u{10FFFF}\u{FDD0}\u{0}\u{7}\u{B}\u{7F}\u{85}";
/// The same as a character, for the cards that take one.
const BAD_CHAR: char = '\u{FFFF}';

fn hostile() -> String {
    HOSTILE.to_owned()
}

fn art() -> Vec<u8> {
    encode_png(&RgbaImage::from_pixel(16, 16, image::Rgba([200, 120, 80, 255]))).expect("encodes")
}

fn is_png(png: &[u8]) -> bool {
    png.starts_with(b"\x89PNG\r\n\x1a\n") && png.len() > 200
}

#[test]
fn hostile_text_renders_on_every_card() {
    let r = Renderer::new();
    let t = Theme::reef;
    let png = art();
    let sprite = || SpriteArt {
        png: &png,
        w: 16,
        h: 16,
    };
    let mut drawn: Vec<(&str, anyhow::Result<Vec<u8>>)> = Vec::new();
    drawn.push((
        "achievements",
        r.achievements_card(&AchievementsCard {
            lang: pinchora_core::i18n::Lang::Tr,
            title: hostile(),
            kicker: hostile(),
            earned_label: hostile(),
            locked_label: hostile(),
            page_label: hostile(),
            empty_label: hostile(),
            cells: vec![AchievementCell {
                name: hostile(),
                description: hostile(),
                icon: "star".into(),
                icon_png: None,
                progress: 1,
                target: 2,
                earned: false,
            }],
            theme: t(),
        }),
    ));

    // Profile, rank, wallet, welcome and level up: names, titles, bios and the partner line.
    let badge = BadgeIcon {
        art: None,
        letter: BAD_CHAR,
        color: 0x06d6a0,
        rarity: 0x3a9bdc,
    };
    let backdrop = || Backdrop {
        art: None,
        accent: "#3a9bdc",
    };
    let slots = [
        ShowcaseSlot {
            kind: ShowcaseKind::Fish,
            art: sprite(),
            name: HOSTILE,
        },
        ShowcaseSlot {
            kind: ShowcaseKind::House,
            art: sprite(),
            name: HOSTILE,
        },
    ];
    let badges = [badge];
    drawn.push((
        "profile",
        r.profile_card(&ProfileCard {
            lang: pinchora_core::i18n::Lang::Tr,
            display_name: HOSTILE,
            title: Some(HOSTILE),
            username: HOSTILE,
            avatar_png: &[],
            bio: HOSTILE,
            level: 7,
            xp_current: 10,
            xp_needed: 100,
            total_xp: 1234,
            credits: 99,
            reputation: 3,
            streak: 4,
            global_rank: 5,
            server_rank: Some(2),
            character: Some(sprite()),
            backdrop: Some(backdrop()),
            badges: &badges,
            showcase: &slots,
            partner: Some(hostile()),
            birthday_today: false,
            theme: t(),
        }),
    ));
    drawn.push((
        "rank",
        r.rank_card(&RankCard {
            lang: pinchora_core::i18n::Lang::Tr,
            display_name: HOSTILE,
            title: Some(HOSTILE),
            username: HOSTILE,
            avatar_png: &[],
            level: 3,
            xp_current: 5,
            xp_needed: 50,
            rank: 8,
            next_role: Some(NextRole {
                name: HOSTILE,
                level: 5,
            }),
            backdrop: Some(backdrop()),
            theme: t(),
        }),
    ));
    drawn.push((
        "wallet",
        r.wallet_card(&WalletCard {
            lang: pinchora_core::i18n::Lang::En,
            display_name: HOSTILE,
            avatar_png: &[],
            shells: 10,
            pearls: 2,
            streak: 3,
            next_daily: HOSTILE,
            next_weekly: HOSTILE,
            next_monthly: HOSTILE,
            pets: 1,
            fish_found: 4,
            fish_total: 60,
            backdrop: Some(backdrop()),
            theme: t(),
        }),
    ));
    drawn.push((
        "welcome",
        r.welcome_card(&WelcomeCard {
            display_name: HOSTILE,
            avatar_png: &[],
            server_name: HOSTILE,
            member_number: Some(12),
            title: HOSTILE,
            lang: pinchora_core::i18n::Lang::En,
            theme: t(),
        }),
    ));
    // A server's own design: hostile names in the placeholders, and hostile text typed into a layer.
    let mut design = templates::template("starlit-banner", CardKind::Welcome).expect("a template");
    if let Some(Content::Text { text, .. }) = design.layers.iter_mut().map(|l| &mut l.content).last() {
        *text = format!("{HOSTILE} {{user}}");
    }
    let fill = CardFill {
        lang: pinchora_core::i18n::Lang::En,
        user: HOSTILE,
        username: HOSTILE,
        server: HOSTILE,
        members: Some(12),
    };
    drawn.push((
        "designed",
        r.designed_card(&DesignedCard {
            design: &design,
            avatar_png: &[],
            upload_png: None,
            fill: &fill,
        }),
    ));
    drawn.push((
        "level up",
        r.level_up_card(&LevelUpCard {
            display_name: HOSTILE,
            avatar_png: &[],
            from: 2,
            to: 3,
            lang: pinchora_core::i18n::Lang::En,
            theme: t(),
        }),
    ));
    let cells = [BadgeCell {
        icon: BadgeIcon {
            art: None,
            letter: BAD_CHAR,
            color: 0x06d6a0,
            rarity: 0x3a9bdc,
        },
        name: HOSTILE,
        rarity: HOSTILE,
        state: BadgeState::ForSale { price: 50 },
    }];
    drawn.push((
        "badge collection",
        r.badge_collection(&BadgeCollectionCard {
            lang: pinchora_core::i18n::Lang::En,
            cells: &cells,
            legend: &[(HOSTILE, 0x3a9bdc)],
            theme: t(),
        }),
    ));
    drawn.push((
        "leaderboard",
        r.leaderboard_card(&LeaderboardCard {
            lang: pinchora_core::i18n::Lang::Tr,
            title: hostile(),
            entries: vec![LeaderboardEntry {
                rank: 1,
                avatar_png: Vec::new(),
                name: hostile(),
                value: hostile(),
                detail: hostile(),
            }],
            page: 1,
            last_week: vec![LastWeekEntry {
                rank: 1,
                name: hostile(),
                value: hostile(),
            }],
            empty: "nobody".to_owned(),
            theme: t(),
        }),
    ));
    drawn.push((
        "poll",
        r.poll_card(&PollCard {
            lang: pinchora_core::i18n::Lang::En,
            number: 3,
            question: hostile(),
            options: vec![
                PollOption {
                    label: hostile(),
                    votes: 3,
                },
                PollOption {
                    label: hostile(),
                    votes: 0,
                },
            ],
            closed: false,
            theme: t(),
        }),
    ));

    drawn.push((
        "colours",
        r.color_card(&ColorCard {
            title: hostile(),
            hint: hostile(),
            cells: vec![
                ColorCell {
                    name: hostile(),
                    rgb: 0xff6b4a,
                },
                ColorCell {
                    name: hostile(),
                    rgb: 0x8fd3ff,
                },
            ],
            selected: Some(1),
            sample: hostile(),
            none: hostile(),
            theme: t(),
        }),
    ));

    // Pets, items and the character editor.
    drawn.push((
        "pet",
        r.pet_card(&PetCard {
            lang: pinchora_core::i18n::Lang::En,
            sprite_png: png.clone(),
            sprite_w: 16,
            sprite_h: 16,
            name: hostile(),
            species: hostile(),
            rarity: hostile(),
            rarity_color: 0x3a9bdc,
            level: 2,
            xp: 5,
            xp_needed: 20,
            needs: [50, 60, 70, 80],
            mood: hostile(),
            personality: hostile(),
            status: PetStatus::Away,
            place: hostile(),
            back: hostile(),
            theme: t(),
        }),
    ));
    drawn.push((
        "nursery",
        r.nursery_card(&NurseryCard {
            lang: pinchora_core::i18n::Lang::En,
            title: hostile(),
            subtitle: hostile(),
            pets: vec![NurseryPet {
                sprite_png: png.clone(),
                sprite_w: 16,
                sprite_h: 16,
                name: hostile(),
                level: 2,
                xp: 0.5,
                needs: [50, 60, 70, 80],
                frame: 0,
                active: true,
                status: PetStatus::Awake,
                badge: hostile(),
            }],
            footer: hostile(),
            theme: t(),
        }),
    ));
    drawn.push((
        "item grid",
        r.item_grid(&ItemGridCard {
            lang: pinchora_core::i18n::Lang::En,
            title: hostile(),
            subtitle: hostile(),
            slots: vec![GridSlot {
                sprite_png: png.clone(),
                sprite_w: 16,
                sprite_h: 16,
                name: hostile(),
                line1: hostile(),
                line2: hostile(),
                bar: Some(0.5),
                frame: 0,
                badge: hostile(),
                shell_price: true,
            }],
            footer: hostile(),
            theme: t(),
        }),
    ));
    drawn.push((
        "character",
        r.character_card(&CharacterCard {
            character: RgbaImage::from_pixel(40, 60, image::Rgba([200, 120, 80, 255])),
            name: hostile(),
            status: (hostile(), true),
            tabs: vec![hostile(), hostile()],
            tab: 0,
            caption: hostile(),
            sections: vec![EditorSection {
                title: hostile(),
                cols: 3,
                cells: vec![EditorCell {
                    art: None,
                    swatch: Some(vec![0xff6b4a]),
                    label: hostile(),
                    note: hostile(),
                    current: true,
                    locked: false,
                }],
            }],
            hint: hostile(),
            theme: t(),
        }),
    ));

    // Collection.
    let entry = || EntryPicture {
        png: png.clone(),
        w: 16,
        h: 16,
    };
    drawn.push((
        "dex",
        r.dex_card(&DexCard {
            title: hostile(),
            tab: hostile(),
            owner: hostile(),
            found: 1,
            total: 10,
            page: 1,
            pages: 2,
            cells: vec![DexCell {
                number: 1,
                name: hostile(),
                art: entry(),
                frame: 0,
                owned: true,
                caught: 2,
            }],
            footer: hostile(),
            lang: pinchora_core::i18n::Lang::En,
            theme: t(),
        }),
    ));
    drawn.push((
        "drop",
        r.drop_card(&DropCard {
            title: hostile(),
            subtitle: hostile(),
            cards: vec![DropCritter {
                art: entry(),
                name: hostile(),
                rarity: hostile(),
                frame: 0,
                habitat: hostile(),
                sparkles: 2,
            }],
            footer: hostile(),
            theme: t(),
        }),
    ));
    drawn.push((
        "bottle",
        r.bottle_card(&BottleCard {
            title: hostile(),
            subtitle: hostile(),
            seed: 7,
            theme: t(),
        }),
    ));
    drawn.push((
        "haul",
        r.haul_card(&HaulCard {
            title: hostile(),
            subtitle: hostile(),
            tiles: vec![HaulTile {
                art: entry(),
                name: hostile(),
                rarity: hostile(),
                frame: 0,
                tag: hostile(),
                line: hostile(),
                flavor: hostile(),
            }],
            footer: hostile(),
            theme: t(),
        }),
    ));
    drawn.push((
        "pot",
        r.pot_card(&PotCard {
            level: 2,
            max_level: 5,
            state: PotState::Soaking {
                left: hostile(),
                fraction: 0.5,
                hours: 3,
            },
            rate: hostile(),
            upgrade: hostile(),
            lang: pinchora_core::i18n::Lang::En,
            theme: t(),
        }),
    ));

    // Board and word games.
    let names = [HOSTILE, HOSTILE];
    drawn.push((
        "connect four",
        r.connect4_card(&Connect4Card {
            lang: pinchora_core::i18n::Lang::En,
            cells: &[0u8; 42],
            names,
            turn: Some(0),
            winner: None,
            win_cells: &[],
            last: None,
            status: HOSTILE,
            pot: 100,
            theme: t(),
        }),
    ));
    drawn.push((
        "checkers",
        r.checkers_card(&CheckersCard {
            lang: pinchora_core::i18n::Lang::En,
            cells: &[0u8; 32],
            names,
            turn: Some(1),
            winner: None,
            last: &[],
            selected: None,
            targets: &[],
            status: HOSTILE,
            pot: 100,
            theme: t(),
        }),
    ));
    drawn.push((
        "chess",
        r.chess_card(&ChessCard {
            lang: pinchora_core::i18n::Lang::En,
            pieces: &[0u8; 64],
            names,
            turn: Some(0),
            winner: None,
            last: None,
            selected: None,
            targets: &[],
            check: None,
            status: HOSTILE,
            pot: 100,
            theme: t(),
        }),
    ));
    let sea = || SeaView {
        name: HOSTILE,
        shots: ".".repeat(100).leak(),
        ships: vec![ShipView {
            cell: 0,
            horizontal: true,
            ship: 0,
            sunk: false,
        }],
        afloat: 5,
    };
    drawn.push((
        "radar",
        r.radar_card(&RadarCard {
            lang: pinchora_core::i18n::Lang::Tr,
            seas: [sea(), sea()],
            turn: Some(0),
            winner: None,
            last: None,
            status: HOSTILE,
            pot: 100,
            theme: t(),
        }),
    ));
    drawn.push((
        "fleet",
        r.fleet_card(&FleetCard {
            lang: pinchora_core::i18n::Lang::Tr,
            title: HOSTILE,
            subtitle: HOSTILE,
            sea: sea(),
            sunk: [false; 5],
            footer: HOSTILE,
            last: None,
            theme: t(),
        }),
    ));
    let (grid, rows, cols) = ([false; 100], vec![vec![1u8]; 10], vec![vec![1u8]; 10]);
    let reveal_cells = [1u8; 100];
    for reveal in [
        None,
        Some(Reveal {
            name: HOSTILE,
            color: "#ff6b4a",
            accent: "#ffffff",
            cells: &reveal_cells,
        }),
    ] {
        drawn.push((
            "picross",
            r.picross_card(&PicrossCard {
                title: HOSTILE,
                subtitle: HOSTILE,
                grid: &grid,
                rows: &rows,
                cols: &cols,
                row_done: [false; 10],
                col_done: [false; 10],
                footer: HOSTILE,
                reveal,
                theme: t(),
            }),
        ));
    }
    let creatures = [sprite(), sprite()];
    drawn.push((
        "tide 2048",
        r.tide2048_card(&Tide2048Card {
            cells: &[1u8; 16],
            score: 10,
            best: 20,
            creatures: &creatures,
            fresh: Some(1),
            status: HOSTILE,
            lang: pinchora_core::i18n::Lang::En,
            theme: t(),
        }),
    ));
    let lines = [vec![
        HangmanLetter::Found(BAD_CHAR),
        HangmanLetter::Missed(BAD_CHAR),
        HangmanLetter::Hidden,
    ]];
    drawn.push((
        "hangman",
        r.hangman_card(&HangmanCard {
            lines: &lines,
            category: HOSTILE,
            wrong: &[BAD_CHAR, 'x'],
            misses: 2,
            status: HOSTILE,
            lang: pinchora_core::i18n::Lang::En,
            theme: t(),
        }),
    ));
    let word = [WordleRow {
        letters: [BAD_CHAR, 'A', 'B', '\u{FFFE}', 'C'],
        marks: [1, 2, 3, 1, 2],
    }];
    drawn.push((
        "wordle",
        r.wordle_card(&WordleCard {
            title: HOSTILE,
            rows: &word,
            keyboard: &[],
            footer: HOSTILE,
            theme: t(),
        }),
    ));

    // Party games.
    let backers = [hostile(), hostile()];
    let lanes = [
        DerbyLane {
            name: HOSTILE,
            accent: 0xff6b4a,
            art: sprite(),
            backers: &backers,
        },
        DerbyLane {
            name: HOSTILE,
            accent: 0x3a9bdc,
            art: sprite(),
            backers: &backers,
        },
    ];
    drawn.push((
        "derby",
        r.derby_card(&DerbyCard {
            lang: pinchora_core::i18n::Lang::En,
            lanes: &lanes,
            stake: 50,
            racers: 2,
            status: HOSTILE,
            theme: t(),
        }),
    ));
    drawn.push((
        "share or grab",
        r.share_grab_card(&ShareGrabCard {
            lang: pinchora_core::i18n::Lang::Tr,
            names,
            picks: [PickView::Locked, PickView::Thinking],
            pool: 100,
            wager: true,
            banner: HOSTILE,
            mood: Mood::Good,
            theme: t(),
        }),
    ));
    drawn.push((
        "shell game",
        r.shell_card(&ShellCard {
            lang: pinchora_core::i18n::Lang::En,
            stake: 10,
            picked: Some(1),
            pearl: Some(2),
            status: HOSTILE,
            payout: 20,
            theme: t(),
        }),
    ));
    let crew = [CrewView { name: HOSTILE, role: 0 }, CrewView { name: HOSTILE, role: 1 }];
    let stage = |passed| StageView {
        title: HOSTILE,
        chance: 60,
        passed,
        line: HOSTILE,
    };
    drawn.push((
        "heist",
        r.heist_card(&HeistCard {
            lang: pinchora_core::i18n::Lang::En,
            crew: &crew,
            stages: [stage(Some(true)), stage(Some(false)), stage(None)],
            headline: HOSTILE,
            each: 30,
            theme: t(),
        }),
    ));
    // A help page: the title and intro, a command and its text, a manager row and a row that wraps to two lines.
    drawn.push((
        "help",
        r.help_card(&HelpCard {
            lang: pinchora_core::i18n::Lang::Tr,
            title: hostile(),
            intro: hostile(),
            page: 2,
            pages: 12,
            rows: vec![
                HelpRow {
                    command: hostile(),
                    text: hostile(),
                    manager: false,
                },
                HelpRow {
                    command: hostile(),
                    text: HOSTILE.repeat(3),
                    manager: true,
                },
            ],
            theme: t(),
        }),
    ));
    let top = [(hostile(), 10i64), (hostile(), 5)];
    drawn.push((
        "storm surge",
        r.surge_card(&SurgeCard {
            lang: pinchora_core::i18n::Lang::En,
            wall: 5,
            goal: 10,
            helpers: 2,
            top: &top,
            state: SurgeState::Rolling,
            theme: t(),
        }),
    ));
    let players = [hostile(), hostile(), hostile()];
    drawn.push((
        "party lobby",
        r.party_lobby_card(&PartyLobbyCard {
            lang: pinchora_core::i18n::Lang::En,
            title: HOSTILE,
            tagline: HOSTILE,
            players: &players,
            min: 2,
            max: 8,
            footer: HOSTILE,
            theme: t(),
        }),
    ));
    let seats = [
        PearlSeat {
            name: HOSTILE,
            lives: 2,
            max_lives: 3,
            out: false,
            current: true,
        },
        PearlSeat {
            name: HOSTILE,
            lives: 0,
            max_lives: 3,
            out: true,
            current: false,
        },
    ];
    drawn.push((
        "hot pearl",
        r.pearl_card(&PearlCard {
            lang: pinchora_core::i18n::Lang::En,
            syllable: HOSTILE,
            seats: &seats,
            banner: HOSTILE,
            last_word: Some(HOSTILE),
            theme: t(),
        }),
    ));
    let standing = [StandingRow {
        rank: 1,
        name: HOSTILE,
        score: HOSTILE,
        note: HOSTILE,
        winner: true,
    }];
    drawn.push((
        "standings",
        r.standings_card(&StandingsCard {
            title: HOSTILE,
            subtitle: HOSTILE,
            rows: &standing,
            footer: HOSTILE,
            theme: t(),
        }),
    ));
    drawn.push((
        "letters",
        r.letters_card(&LettersCard {
            title: HOSTILE,
            round: HOSTILE,
            letters: &[BAD_CHAR, 'A', '\u{FFFE}'],
            hint: HOSTILE,
            status: HOSTILE,
            theme: t(),
        }),
    ));
    let entry_rows = [
        EntryRow {
            text: HOSTILE,
            author: Some(HOSTILE),
            votes: Some(2),
            winner: true,
        },
        EntryRow {
            text: HOSTILE,
            author: None,
            votes: None,
            winner: false,
        },
    ];
    drawn.push((
        "entries",
        r.entries_card(&EntriesCard {
            lang: pinchora_core::i18n::Lang::En,
            title: HOSTILE,
            subtitle: HOSTILE,
            chip: HOSTILE,
            header: Some(EntryHeader {
                png: &png,
                w: 16,
                h: 16,
            }),
            rows: &entry_rows,
            footer: HOSTILE,
            theme: t(),
        }),
    ));
    let guesses = [[1u8, 2, 3, 1, 2]];
    let racers = [
        Racer {
            name: HOSTILE,
            rows: &guesses,
            state: RacerState::Solved(1),
        },
        Racer {
            name: HOSTILE,
            rows: &guesses,
            state: RacerState::Failed,
        },
    ];
    drawn.push((
        "wordle race",
        r.race_card(&RaceCard {
            lang: pinchora_core::i18n::Lang::En,
            title: HOSTILE,
            racers: &racers,
            footer: HOSTILE,
            theme: t(),
        }),
    ));
    drawn.push((
        "postcard",
        r.postcard_card(&PostcardCard {
            lang: pinchora_core::i18n::Lang::En,
            scene: &png,
            place: HOSTILE,
            stamp: HOSTILE,
            theme: t(),
        }),
    ));
    drawn.push((
        "would you rather",
        r.rather_card(&RatherCard {
            lang: pinchora_core::i18n::Lang::En,
            category: HOSTILE,
            a: HOSTILE,
            b: HOSTILE,
            votes: Some((3, 4)),
            closed: true,
            theme: t(),
        }),
    ));
    for tone in [PromptTone::Turn, PromptTone::Truth, PromptTone::Dare] {
        drawn.push((
            "truth or dare",
            r.prompt_card(&PromptCard {
                lang: pinchora_core::i18n::Lang::En,
                tone,
                name: HOSTILE,
                text: HOSTILE,
                category: HOSTILE,
                footer: HOSTILE,
                theme: t(),
            }),
        ));
    }

    // Wrapped.
    drawn.push((
        "wrapped",
        r.wrapped_card(&WrappedCard {
            lang: pinchora_core::i18n::Lang::Tr,
            year: 2026,
            display_name: HOSTILE,
            avatar_png: &[],
            title: HOSTILE,
            totals: WrappedTotals {
                messages: 1,
                xp: 2,
                shells: 3,
                fish: 4,
                visits: 5,
                likes_given: 6,
                likes_received: 7,
                pet_care: 8,
            },
            months: [1; 12],
            fish: Some(WrappedFish {
                name: HOSTILE,
                caught: 3,
                art: Some(sprite()),
            }),
            house: Some(WrappedHouse {
                name: HOSTILE,
                visits: 4,
                friend: true,
            }),
            backdrop: Some(backdrop()),
            theme: t(),
        }),
    ));

    drawn.push((
        "quick",
        r.quick_card(&QuickCard {
            lang: pinchora_core::i18n::Lang::Tr,
            title: HOSTILE,
            subtitle: HOSTILE,
            cells: &[],
            columns: 0,
            prompt: HOSTILE,
            status: HOSTILE,
            theme: t(),
            distort: true,
        }),
    ));

    // The raw SVG path every card ends in: code points XML forbids that never went through `escape`.
    drawn.push((
        "raw svg",
        r.svg_to_png(&format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="32"><text x="2" y="20" font-size="12" data-x="a{FORBIDDEN}b">Zoë ✨ 李 😀 {FORBIDDEN}</text></svg>"#
        )),
    ));

    let failures: Vec<String> = drawn
        .iter()
        .filter_map(|(card, result)| match result {
            Ok(png) if is_png(png) => None,
            Ok(_) => Some(format!("{card}: not a picture")),
            Err(e) => Some(format!("{card}: {e}")),
        })
        .collect();
    assert!(failures.is_empty(), "cards that did not draw:\n{}", failures.join("\n"));
    // A card added to the renderer without a case above fails here.
    let methods = include_str!("../src/lib.rs").matches("(&self, card: &").count();
    let covered: std::collections::BTreeSet<_> = drawn
        .iter()
        .map(|(card, _)| *card)
        .filter(|c| *c != "raw svg")
        .collect();
    assert_eq!(covered.len(), methods, "one case per card method: {covered:?}");
}
