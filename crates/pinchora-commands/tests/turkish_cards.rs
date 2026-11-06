//! Turkish content names are longer than the English ones and use letters (ç ğ ı ö ş ü İ) that the pixel font has
//! to draw itself (Silkscreen got İ ı Ş ş Ğ ğ for them). These tests draw the cards that show content names with
//! the longest Turkish names, and the profile, help, wallet and wordle cards in Turkish, and save the PNGs under
//! `target/sim/turkish_cards/` so they can be looked at.

use std::path::Path;

use image::{Rgba, RgbaImage};
use pinchora_core::{i18n::Lang, t};
use pinchora_game::{Content, lang, overlay::upper};
use pinchora_render::{
    CORRECT, GridSlot, HelpCard, HelpRow, ItemGridCard, PRESENT, ProfileCard, Renderer, Theme, WalletCard, WordleCard,
    WordleRow, encode_png, placeholder_avatar,
};

fn out(name: &str) -> std::path::PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .parent()
        .expect("target dir")
        .join("sim")
        .join("turkish_cards");
    std::fs::create_dir_all(&dir).expect("create the output dir");
    dir.join(name)
}

fn is_png(png: &[u8]) -> bool {
    png.starts_with(b"\x89PNG\r\n\x1a\n") && png.len() > 2_000
}

fn blank_sprite() -> Vec<u8> {
    encode_png(&RgbaImage::from_pixel(96, 88, Rgba([200, 150, 110, 255]))).expect("encodes")
}

/// The eight items whose Turkish names are the longest, as the shop catalogue would show them.
#[test]
fn the_item_grid_holds_the_longest_turkish_names() {
    let content = Content::get();
    let mut items: Vec<_> = content.items().collect();
    items.sort_by_key(|i| std::cmp::Reverse(i.name_in(Lang::Tr).chars().count()));
    let slots: Vec<GridSlot> = items
        .iter()
        .take(8)
        .map(|i| GridSlot {
            sprite_png: blank_sprite(),
            sprite_w: 96,
            sprite_h: 88,
            name: i.name_in(Lang::Tr),
            line1: "12.500".to_owned(),
            line2: format!(
                "{} · {}×{}",
                upper(i.base.rarity.label_in(Lang::Tr), Lang::Tr),
                i.base.footprint[0],
                i.base.footprint[1]
            ),
            bar: None,
            frame: i.base.rarity.color(),
            badge: upper(i.base.category.label_in(Lang::Tr), Lang::Tr),
            shell_price: true,
        })
        .collect();
    let card = ItemGridCard {
        title: format!("{} katalog", pinchora_game::Category::WindowsDoors.label_in(Lang::Tr)),
        subtitle: "SAYFA 1/3".to_owned(),
        slots,
        footer: "BU REYONDA 24 ÜRÜN VAR".to_owned(),
        theme: Theme::reef(),
    };
    let png = Renderer::new().item_grid(&card).expect("renders");
    assert!(is_png(&png));
    std::fs::write(out("item_grid_longest.png"), png).expect("write png");
}

/// Every pixel letter of the Turkish alphabet at the sizes the cards draw them, both weights.
#[test]
fn the_pixel_font_specimen_shows_the_whole_turkish_alphabet() {
    let line = |y: u32, size: u32, bold: bool, text: &str| {
        let weight = if bold { "bold" } else { "normal" };
        format!(
            r##"<text x="24" y="{y}" font-family="Silkscreen" font-weight="{weight}" font-size="{size}" fill="#e8eeff">{text}</text>"##
        )
    };
    let caps = lang::upper(Lang::Tr, "abcçdefgğhıijklmnoöprsştuüvyz");
    let words = lang::upper(Lang::Tr, "şirin ığdır çağrı öğle ünlü kâr hâlâ içkili");
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="1000" height="420"><rect width="1000" height="420" fill="#141b3d"/>{}{}{}{}{}{}</svg>"##,
        line(56, 32, false, &caps),
        line(112, 32, true, &caps),
        line(168, 24, false, &words),
        line(220, 24, true, &words),
        line(272, 16, false, &format!("{caps} {words}")),
        line(320, 16, true, &format!("{caps} {words}")),
    );
    let png = Renderer::new().svg_to_png(&svg).expect("renders");
    assert!(is_png(&png));
    std::fs::write(out("pixel_alphabet.png"), png).expect("write png");
}

fn turkish_profile<'a>(avatar: &'a [u8]) -> ProfileCard<'a> {
    ProfileCard {
        lang: Lang::Tr,
        display_name: "Şirin Çağlayan",
        title: Some("Şampiyon inci avcısı"),
        username: "sirin.caglayan",
        avatar_png: avatar,
        bio: "Kâşif, çiçek ve öğle sıcağı sever. İyi bir günün şerefine!",
        level: 7,
        xp_current: 120,
        xp_needed: 400,
        total_xp: 2310,
        credits: 12_500,
        reputation: 14,
        streak: 5,
        global_rank: 42,
        server_rank: Some(3),
        character: None,
        backdrop: None,
        badges: &[],
        showcase: &[],
        partner: Some("ığdırlı Öğüt".to_owned()),
        theme: Theme::reef(),
    }
}

/// The profile card: SEVİYE, DENEYİM, İNCİ, SERİ and EVCİL from the catalog, and a title and a partner the card
/// capitalises itself, with Turkish `i` and `ı`.
#[test]
fn the_turkish_profile_card_draws_its_capitals() {
    let avatar = placeholder_avatar(7, ProfileCard::AVATAR_SIZE);
    let card = turkish_profile(&avatar);
    let svg = card.to_svg();
    for label in ["SEVİYE", "DENEYİM", "ŞAMPİYON İNCİ AVCISI", "IĞDIRLI ÖĞÜT"] {
        assert!(svg.contains(label), "{label} is on the card");
    }
    // The text reaches the renderer as it is: nothing is turned into the plain ASCII letter.
    assert!(!svg.contains("SEVIYE") && !svg.contains("SAMPIYON"));
    let png = Renderer::new().profile_card(&card).expect("renders");
    assert!(is_png(&png));
    std::fs::write(out("profile_tr.png"), png).expect("write png");
}

#[test]
fn the_turkish_wallet_card_draws_its_capitals() {
    let avatar = placeholder_avatar(9, WalletCard::AVATAR_SIZE);
    let (hours, minutes) = (t!(Lang::Tr, "time.hours", n = 5), t!(Lang::Tr, "time.minutes", n = 20));
    let weekly = format!("{hours} {minutes}");
    let monthly = t!(Lang::Tr, "time.days", n = 12);
    let card = WalletCard {
        lang: Lang::Tr,
        display_name: "Şirin Çağlayan",
        avatar_png: &avatar,
        shells: 12_500,
        pearls: 40,
        streak: 5,
        next_daily: &t!(Lang::Tr, "time.ready"),
        next_weekly: &weekly,
        next_monthly: &monthly,
        pets: 3,
        fish_found: 21,
        fish_total: 60,
        backdrop: None,
        theme: Theme::reef(),
    };
    let svg = card.to_svg();
    for label in ["CÜZDAN", "İNCİ", "SERİ", "EVCİL", "GÜNLÜK", "HAFTALIK", "HAZIR"] {
        assert!(svg.contains(label), "{label} is on the card");
    }
    let png = Renderer::new().wallet_card(&card).expect("renders");
    assert!(is_png(&png));
    std::fs::write(out("wallet_tr.png"), png).expect("write png");
}

#[test]
fn the_turkish_help_card_draws_its_capitals() {
    let card = HelpCard {
        lang: Lang::Tr,
        title: "Ev ve süsleme".to_owned(),
        intro: "Odaları döşe, arkadaşlarını ziyaret et ve eşyalarını göster.".to_owned(),
        page: 2,
        pages: 12,
        rows: vec![
            HelpRow {
                command: "/ev düzenle".to_owned(),
                text: "Evini düzenle: eşyaları taşı, çevir ve yerleştir.".to_owned(),
                manager: false,
            },
            HelpRow {
                command: "/ayarlar dil".to_owned(),
                text: "Sunucunun varsayılan dilini seç.".to_owned(),
                manager: true,
            },
        ],
        theme: Theme::reef(),
    };
    let png = Renderer::new().help_card(&card).expect("renders");
    assert!(is_png(&png));
    std::fs::write(out("help_tr.png"), png).expect("write png");
}

/// The wordle board with the Turkish keyboard: Ğ Ü / Ş İ / Ö Ç are keys, and a guess shows İ, Ş, Ğ, Ö, Ü, Ç and I.
#[test]
fn the_turkish_wordle_card_draws_the_turkish_keyboard() {
    let word = |w: &str, marks: [u8; 5]| WordleRow {
        letters: w
            .chars()
            .map(|c| lang::upper_char(Lang::Tr, c))
            .collect::<Vec<_>>()
            .try_into()
            .expect("five letters"),
        marks,
    };
    let rows = [
        word("çiğşü", [0, CORRECT, PRESENT, 1, 1]),
        word("ışıkö", [PRESENT, 1, CORRECT, 1, PRESENT]),
        word("iğdır", [CORRECT, 1, 1, PRESENT, 1]),
    ];
    let keyboard: Vec<Vec<(char, u8)>> = lang::keyboard_rows(Lang::Tr)
        .iter()
        .map(|row| {
            row.chars()
                .map(|c| {
                    let mark = match c {
                        'i' | 'ğ' => CORRECT,
                        'ı' | 'ş' | 'ö' => PRESENT,
                        'ç' | 'ü' => 1,
                        _ => 0,
                    };
                    (lang::upper_char(Lang::Tr, c), mark)
                })
                .collect()
        })
        .collect();
    let (title, footer) = (
        lang::upper(Lang::Tr, "Günlük wordle #12"),
        lang::upper(Lang::Tr, "Deneme 4 / 6"),
    );
    let card = WordleCard {
        title: &title,
        rows: &rows,
        keyboard: &keyboard,
        footer: &footer,
        theme: Theme::reef(),
    };
    let svg = card.to_svg();
    for letter in [">İ<", ">Ş<", ">Ğ<", ">Ö<", ">Ü<", ">Ç<", ">I<"] {
        assert!(svg.contains(letter), "{letter} is a tile or a key");
    }
    let png = Renderer::new().wordle_card(&card).expect("renders");
    assert!(is_png(&png));
    std::fs::write(out("wordle_tr.png"), png).expect("write png");
}
