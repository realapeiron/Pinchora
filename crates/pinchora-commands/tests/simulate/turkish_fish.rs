//! Turkish for the fish group, played through the real handlers.

use pinchora_core::infra::i18n::Lang;
use pinchora_game::lang::lower;

use crate::*;

/// The label of the first button that starts with `prefix`.
fn label_starting(reply: &Reply, prefix: &str) -> String {
    reply
        .controls()
        .find_map(|c| match c {
            Component::Button(b) => b.label.clone().filter(|l| l.starts_with(prefix)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no button starting {prefix:?}"))
}

/// The nonce of the cast a reel button belongs to (`fr:<user>:<nonce>`).
fn nonce_of(reply: &Reply, label: &str) -> String {
    let id = reply.button(label).custom_id.clone().expect("a custom id");
    id.rsplit(':').next().expect("a nonce").to_owned()
}

#[tokio::test]
async fn a_turkish_angler_shops_gears_casts_reels_sells_and_reads_the_book() {
    let Some(mut sim) = Sim::new("a_turkish_angler_shops_gears_casts_reels_sells_and_reads_the_book").await else {
        return;
    };
    let db = sim.app.db.clone();
    let content = Content::get();
    let angler = unique();
    fund(&sim, angler, 1_000).await;
    sim.locale = "tr";

    // The tackle shop: Turkish names, blurbs and menu texts; the rods and baits are the same ones.
    let shop = sim.command(angler, "fish", fish_sub("shop")).await;
    assert!(shop.ephemeral);
    assert_eq!(shop.title(), "🎣 Olta dükkânı");
    assert!(
        shop.description().starts_with("Oltalar bir kez alınır")
            && shop.description().contains("Gümüş Yem ve Gelgit Kırıntıları")
            && shop.description().contains("Abis yerinde"),
        "{}",
        shop.description()
    );
    assert_eq!(
        shop.menu("fx:p:").placeholder.as_deref(),
        Some("Bir olta ya da yem paketi al…")
    );
    let options = shop.options("fx:p:");
    assert_eq!(options[0].value, "r:reedline_rod");
    assert_eq!(options[0].label, "Reedline Oltası");
    assert_eq!(
        options[0].description.as_deref(),
        Some("800 kabuk · çekme süresi 2,8 sn · nadir balık şansı +6%")
    );
    let worm = options.iter().find(|o| o.value == "b:glow_worm").expect("worms");
    assert_eq!(worm.label, "Işıklı Solucan (paket: 5)");
    assert!(
        worm.description
            .as_deref()
            .is_some_and(|d| d.starts_with("65 kabuk · ") && d.contains("Seyrek balıklar")),
        "{:?}",
        worm.description
    );

    // Buying: a rod, too dear a rod, the same rod twice, a pack of bait, bait that costs too much.
    let bought = sim.pick(angler, &shop, "fx:p:", "r:reedline_rod").await;
    assert!(
        bought
            .description()
            .starts_with("**Reedline Oltası** 800 kabuğa alındı. Takmak için `/fish gear`"),
        "{}",
        bought.description()
    );
    assert_eq!(shells(&sim, angler).await, 200);
    let poor = sim.pick(angler, &bought, "fx:p:", "r:tidecaller_rod").await;
    assert_eq!(poor.description(), "**Tidecaller Oltası** 12.000 kabuk; sende 200 var.");
    let again = sim
        .component(
            angler,
            &bought.menu("fx:p:").custom_id.clone(),
            &["r:reedline_rod"],
            &bought,
        )
        .await;
    assert_eq!(again.description(), "**Reedline Oltası** zaten sende.");
    let worms = sim.pick(angler, &bought, "fx:p:", "b:glow_worm").await;
    assert_eq!(
        worms.description(),
        "5 adet **Işıklı Solucan** 65 kabuğa alındı; sende 5 var. Takmak için `/fish gear` kullan."
    );
    let broke = sim.pick(angler, &worms, "fx:p:", "b:silver_lure").await;
    assert_eq!(broke.description(), "**Gümüş Yem** paketi 140 kabuk; sende 135 var.");
    let nothing = sim
        .component(angler, &broke.menu("fx:p:").custom_id.clone(), &["r:gold_rod"], &broke)
        .await;
    assert_eq!(nothing.description(), "O olta dükkânda yok.");
    let stranger = sim
        .component(
            unique(),
            &broke.menu("fx:p:").custom_id.clone(),
            &["b:glow_worm"],
            &broke,
        )
        .await;
    assert!(
        stranger.content().starts_with("Bu başkasının paneli."),
        "{}",
        stranger.content()
    );
    assert_eq!(shells(&sim, angler).await, 135);

    // The gear panel: fields, menus, the closed spots with the level they need.
    let gear = sim.command(angler, "fish", fish_sub("gear")).await;
    assert!(gear.ephemeral);
    assert_eq!(gear.title(), "🎣 Olta takımın");
    assert_eq!(gear.description(), "Menülerden seç; bir sonraki atış bunları kullanır.");
    assert!(gear.field("Yer").starts_with("**Kıyı**\n"), "{}", gear.field("Yer"));
    assert!(
        gear.field("Olta").starts_with("**Deniz Odunu Oltası**\n"),
        "{}",
        gear.field("Olta")
    );
    assert_eq!(gear.field("Yem"), "**Sade yem**\nAtış başı 10 kabuk");
    assert_eq!(
        gear.embeds[0].footer.as_ref().map(|f| f.text.as_str()),
        Some("Seviye 0 · Resif 5. seviyede açılıyor")
    );
    assert_eq!(
        gear.menu("fx:s:").placeholder.as_deref(),
        Some("Nerede balık tutacaksın…")
    );
    assert_eq!(gear.menu("fx:r:").placeholder.as_deref(), Some("Hangi olta…"));
    assert_eq!(gear.menu("fx:b:").placeholder.as_deref(), Some("Hangi yem…"));
    let spots: Vec<_> = gear
        .options("fx:s:")
        .iter()
        .map(|o| (o.label.as_str(), o.description.as_deref().unwrap_or_default()))
        .collect();
    assert_eq!(
        spots.iter().map(|(label, _)| *label).collect::<Vec<_>>(),
        ["Kıyı", "🔒 Resif", "🔒 Yosun Ormanı", "🔒 Derin Deniz", "🔒 Abis"]
    );
    assert_eq!(spots[3].1, "20. seviyede açılır");
    assert!(spots[0].1.starts_with("Sıcak kum"), "{}", spots[0].1);
    let labels = |reply: &Reply, prefix: &str| -> Vec<String> {
        reply.options(prefix).iter().map(|o| o.label.clone()).collect()
    };
    assert_eq!(labels(&gear, "fx:r:"), ["Deniz Odunu Oltası", "Reedline Oltası"]);
    assert_eq!(labels(&gear, "fx:b:"), ["Sade yem", "Işıklı Solucan ×5"]);

    let refused = sim.pick(angler, &gear, "fx:s:", "deep_sea").await;
    assert_eq!(
        refused.description(),
        "**Derin Deniz** 20. seviyede açılıyor, senin seviyen 0."
    );
    let rod = sim.pick(angler, &refused, "fx:r:", "reedline_rod").await;
    assert_eq!(rod.description(), "**Reedline Oltası** takıldı.");
    assert!(
        rod.field("Olta")
            .contains("çekme süresi 2,8 sn · nadir balık şansı +6%"),
        "{}",
        rod.field("Olta")
    );
    let hook = sim.pick(angler, &rod, "fx:b:", "glow_worm").await;
    assert_eq!(hook.description(), "**Işıklı Solucan** kancaya takıldı.");
    assert!(
        hook.field("Yem").starts_with("**Işıklı Solucan** ×5\n"),
        "{}",
        hook.field("Yem")
    );
    let fake = sim
        .component(angler, &hook.menu("fx:r:").custom_id.clone(), &["starweave_rod"], &hook)
        .await;
    assert_eq!(
        fake.description(),
        "Starweave Oltası henüz sende değil. `/fish shop` ile satın al."
    );
    let plain = sim.pick(angler, &hook, "fx:b:", "plain").await;
    assert_eq!(plain.description(), "Sade yem takıldı: atış başı 10 kabuk.");
    let _ = sim.pick(angler, &plain, "fx:b:", "glow_worm").await;
    assert_eq!(
        db.fishing_gear(angler).await.unwrap().bait.as_deref(),
        Some("glow_worm")
    );

    // Casting: a spot that is still shut, then the real cast with a worm on the hook.
    let shut = sim.command(angler, "fish", fish_cast(Some("reef"))).await;
    assert_eq!(
        shut.content(),
        "**Resif** 5. seviyede açılıyor, senin seviyen 0. Şimdilik **Kıyı** senin."
    );
    let cast = sim.command(angler, "fish", fish_cast(None)).await;
    assert_eq!(cast.title(), "🎣 Olta suda…");
    assert!(
        cast.description().contains("**Çek!** düğmesine bas"),
        "{}",
        cast.description()
    );
    assert_eq!(footer(&cast), "Kıyı · Reedline Oltası · Işıklı Solucan ×4 kaldı");
    assert!(!cast.button("Çek!").disabled);
    assert_eq!(shells(&sim, angler).await, 135, "the worm paid for the cast");
    let cooling = sim.command(angler, "fish", fish_cast(None)).await;
    assert!(
        cooling.content().starts_with("Misinan hâlâ kuruyor. ") && cooling.content().ends_with(" sn sonra yine at."),
        "{}",
        cooling.content()
    );

    // The bite arrives on its own, in the language the cast was made in (the task has no interaction).
    let nonce = nonce_of(&cast, "Çek!");
    let state = sim
        .app
        .cache
        .get(&format!("cast:{angler}:{nonce}"))
        .await
        .unwrap()
        .expect("the cast is waiting");
    let bite_at: i64 = state
        .split(':')
        .next()
        .and_then(|v| v.parse().ok())
        .expect("a bite time");
    let wait = bite_at - chrono::Utc::now().timestamp_millis() + 250;
    tokio::time::sleep(std::time::Duration::from_millis(wait.max(0) as u64)).await;
    let bite_title = {
        let calls = sim.calls.lock().unwrap();
        calls
            .iter()
            .filter(|c| c.method == Method::PATCH && c.path.ends_with("/messages/@original"))
            .filter_map(|c| c.body["embeds"][0]["title"].as_str().map(str::to_owned))
            .next_back()
    };
    assert_eq!(bite_title.as_deref(), Some("❗ Balık yemi kaptı! HEMEN ÇEK!"));

    // Reeling in time: the catch, its note, size, value and footer, all in Turkish.
    let catch = sim.click(angler, &cast, "Çek!").await;
    let bucket = db.bucket(angler).await.unwrap();
    assert_eq!(bucket.len(), 1);
    let fish = content.fish(&bucket[0].fish_id).expect("a known fish");
    let name = content.name(&fish.id, Lang::Tr);
    let rarity = lower(Lang::Tr, fish.rarity.label_in(Lang::Tr));
    assert_eq!(
        catch.title(),
        format!("🆕 Balık yakaladın: {rarity} bir {name}!"),
        "{}",
        catch.title()
    );
    assert_eq!(catch.files[0].0, "catch.png");
    let size = bucket[0].size_cm;
    let flavor = content.description(&fish.id, Lang::Tr).expect("a Turkish flavour");
    let value = pinchora_game::fishing::sale_value(fish, size);
    assert!(
        catch.description().starts_with(&format!(
            "*{flavor}*\n**{} cm** · kovandaki değeri **",
            pinchora_core::admin::server_stats::decimal_in(Lang::Tr, size, 1)
        )),
        "{}",
        catch.description()
    );
    assert!(
        catch.description().contains("Balık defterin için yeni bir tür!"),
        "{}",
        catch.description()
    );
    assert!(
        footer(&catch).starts_with("Kıyı · Tepki 0,")
            && footer(&catch).ends_with("sn · Balık defteri 1/72 · Kova 1/60"),
        "{}",
        footer(&catch)
    );
    assert_eq!(catch.components.len(), 0, "the reel button is gone");

    // The ways a reel goes wrong.
    let slow = unique();
    let id = seed_cast(&sim, slow, 1, 2_600, "").await;
    let missed = sim.component(slow, &id, &[], &casting()).await;
    assert_eq!(missed.title(), "Çok yavaş!");
    assert_eq!(
        missed.description(),
        "Balık kancadan kurtuldu. `/fish cast` ile yeniden at."
    );
    let id = seed_cast(&sim, slow, 2, -800, "shore:driftwood_rod:-").await;
    let early = sim.component(slow, &id, &[], &casting()).await;
    assert_eq!(early.title(), "Çok erken!");
    assert!(
        early.description().starts_with("Su sıçraması"),
        "{}",
        early.description()
    );
    let id = seed_cast(&sim, slow, 3, 100, "").await;
    let theirs = sim.component(unique(), &id, &[], &casting()).await;
    assert_eq!(
        theirs.content(),
        "Bu başkasının oltası! Kendi oltanı `/fish cast` ile at."
    );
    let gone = sim.component(slow, &format!("fr:{slow}:424242"), &[], &casting()).await;
    assert_eq!(
        gone.content(),
        "Bu atış artık geçerli değil. `/fish cast` ile yeniden at."
    );
    let penniless = unique();
    let none = sim.command(penniless, "fish", fish_cast(None)).await;
    assert_eq!(none.content(), "Sade yem 10 kabuk. `/daily` komutunu dene!");

    // Book milestones: a badge to wear, a reward already owned, then the whole book with its backgrounds.
    let tenth = unique();
    for n in 0..9 {
        db.log_catch(tenth, &format!("old{n}"), 5.0).await.unwrap();
    }
    let id = seed_cast(&sim, tenth, 4, 400, "shore:driftwood_rod:-").await;
    let milestone = sim.component(tenth, &id, &[], &casting()).await;
    assert_eq!(milestone.embeds.len(), 2, "the catch and the milestone");
    assert_eq!(
        milestone.embeds[1].title.as_deref(),
        Some("📖 Balık defteri: 10 tür tamamlandı!")
    );
    assert_eq!(
        milestone.embeds[1].description.as_deref(),
        Some("**+300 kabuk** ve **Cep Balon Balığı** rozeti kazandın! `/customize badges` ile takabilirsin.")
    );
    let owner = unique();
    fund(&sim, owner, 5_000).await;
    db.buy_badge(owner, "pocket_puffer", 2_000).await.unwrap();
    for n in 0..9 {
        db.log_catch(owner, &format!("old{n}"), 5.0).await.unwrap();
    }
    let id = seed_cast(&sim, owner, 5, 400, "shore:driftwood_rod:-").await;
    let had = sim.component(owner, &id, &[], &casting()).await;
    assert_eq!(
        had.embeds[1].description.as_deref(),
        Some("**+300 kabuk** ve **Cep Balon Balığı** rozeti, ama o zaten sendeydi.")
    );
    let complete = unique();
    for n in 0..71 {
        db.log_catch(complete, &format!("old{n}"), 5.0).await.unwrap();
    }
    let id = seed_cast(&sim, complete, 6, 400, "shore:driftwood_rod:-").await;
    let all = sim.component(complete, &id, &[], &casting()).await;
    assert_eq!(all.embeds.len(), 5);
    assert_eq!(
        all.embeds[2].description.as_deref(),
        Some(
            "**+1.000 kabuk** ve **Brinebell Pazarı** profil arka planı kazandın! `/customize background` ile takabilirsin."
        )
    );

    // Selling: the bucket card, the buttons, the sale, the empty bucket.
    let bucket_panel = sim.command(angler, "fish", fish_sub("sell")).await;
    assert!(bucket_panel.ephemeral);
    assert_eq!(bucket_panel.title(), "🪣 Balık kovan");
    assert!(
        bucket_panel
            .description()
            .starts_with("Hepsini sat ya da her türün en büyüğünü sakla"),
        "{}",
        bucket_panel.description()
    );
    let sell_all = label_starting(&bucket_panel, "Hepsini sat (");
    assert!(
        sell_all.starts_with("Hepsini sat (1 · ") && sell_all.ends_with(" kabuk)"),
        "{sell_all}"
    );
    assert!(
        bucket_panel
            .button(&label_starting(&bucket_panel, "Kopyaları sat ("))
            .disabled
    );
    let stranger = sim.click(unique(), &bucket_panel, &sell_all).await;
    assert_eq!(
        stranger.content(),
        "Bu başkasının kovası. Kendininkini `/fish sell` ile aç."
    );
    let before = shells(&sim, angler).await;
    let sold = sim.click(angler, &bucket_panel, &sell_all).await;
    assert!(
        sold.description().starts_with("**1 balık** sattın, karşılığında **")
            && sold.description().contains("kabuk** kazandın. Artık "),
        "{}",
        sold.description()
    );
    assert_eq!(shells(&sim, angler).await, before + value);
    let empty = sim.command(angler, "fish", fish_sub("sell")).await;
    assert_eq!(empty.description(), "Kova boş. `/fish cast` ile olta at!");
    let nothing = sim.component(angler, &format!("fz:s:{angler}:all"), &[], &sold).await;
    assert_eq!(nothing.description(), "Şu an satılacak bir şey yok.");
    let expired = sim.component(angler, &format!("fz:x:{angler}"), &[], &sold).await;
    assert_eq!(expired.content(), "Bu düğmenin süresi doldu.");

    // A full bucket of several species: the card, the refused cast, selling the duplicates.
    let hoarder = unique();
    fund(&sim, hoarder, 100).await;
    let species: Vec<_> = content.fish.iter().step_by(9).take(8).collect();
    for n in 0..60 {
        let fish = species[n % species.len()];
        db.record_catch(
            hoarder,
            pinchora_db::NewCatch {
                fish_id: &fish.id,
                size_cm: fish.min_cm + (n as f32) * 0.1,
                ratio: 1.0,
                weekly: None,
                milestones: &[],
                known: &known_fish(),
            },
        )
        .await
        .unwrap();
    }
    let full = sim.command(hoarder, "fish", fish_cast(None)).await;
    assert_eq!(
        full.content(),
        "Kovan dolu (60/60). Önce `/fish sell` ile birkaç balık sat."
    );
    let crowded = sim.command(hoarder, "fish", fish_sub("sell")).await;
    assert!(crowded.files[0].0 == "bucket.png");
    let duplicates = sim
        .click(hoarder, &crowded, &label_starting(&crowded, "Kopyaları sat ("))
        .await;
    assert!(
        duplicates.description().starts_with("**52 balık** sattın"),
        "{}",
        duplicates.description()
    );

    // The fish book: rewards list, page buttons, somebody else's book.
    for (n, fish) in content.fish.iter().take(12).enumerate() {
        db.log_catch(
            hoarder,
            &fish.id,
            fish.min_cm + (fish.max_cm - fish.min_cm) * (n as f32 / 12.0),
        )
        .await
        .unwrap();
    }
    let book = sim.command(hoarder, "fish", fish_sub("book")).await;
    assert!(!book.ephemeral);
    assert_eq!(book.title(), "📖 Balık defteri");
    let lines: Vec<_> = book.description().lines().collect();
    assert_eq!(lines.len(), 4);
    assert!(
        lines[0].starts_with("✅ **10 tür**: 300 kabuk ve Cep Balon Balığı rozeti"),
        "{lines:?}"
    );
    assert!(lines[3].ends_with("Moonwhale Geçidi arka planı"), "{lines:?}");
    assert!(book.button("◀ Sayfa").disabled && !book.button("Sayfa ▶").disabled);
    let page_2 = sim.click(hoarder, &book, "Sayfa ▶").await;
    assert!(!page_2.button("◀ Sayfa").disabled);
    assert_eq!(page_2.title(), "📖 Balık defteri");
    let stranger = sim.click(unique(), &page_2, "◀ Sayfa").await;
    assert_eq!(
        stranger.content(),
        "Bu başkasının defteri. Kendininkini `/fish book` ile aç."
    );

    // Everything above is Turkish; the same screens in English stay as they were.
    sim.locale = "en-US";
    let reader = unique();
    fund(&sim, reader, 50).await;
    let english = sim.command(reader, "fish", fish_sub("shop")).await;
    assert_eq!(english.title(), "🎣 Tackle shop");
    assert!(
        english
            .description()
            .ends_with("Tide Crumbs cuts the wage at the Abyss."),
        "{}",
        english.description()
    );
    let casting_en = sim.command(reader, "fish", fish_cast(None)).await;
    assert_eq!(casting_en.title(), "🎣 Line in the water…");
    assert_eq!(footer(&casting_en), "Shore · Driftwood Rod · Plain bait");
    assert!(casting_en.has_button("Reel!"));

    // The remaining bites land before the test ends.
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
}
