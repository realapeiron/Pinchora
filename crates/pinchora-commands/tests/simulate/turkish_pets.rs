//! Pets, the nursery, field trips and the card editors (titles, style, showcase) for a player whose Discord is
//! set to Turkish.

use pinchora_core::i18n::Lang;

use super::*;

/// Keeps the picture of a reply next to the sim's own, under a name that says what it shows.
fn keep_png(sim: &Sim, reply: &Reply, name: &str) {
    let (_, png) = reply.files.first().expect("the reply carried a picture");
    std::fs::write(sim.dir.join(format!("{name}.png")), png).expect("save the picture");
}

#[tokio::test]
async fn a_turkish_player_adopts_cares_for_and_travels_with_a_pet() {
    let Some(mut sim) = Sim::new("a_turkish_player_adopts_cares_for_and_travels_with_a_pet").await else {
        return;
    };
    let (user, db, app) = (unique(), sim.app.db.clone(), sim.app.clone());
    let content = Content::get();
    sim.locale = "tr";

    let none = sim.command(user, "pet", pet_command("view")).await;
    assert!(
        none.ephemeral && none.content().contains("Henüz bir hayvanın yok"),
        "{}",
        none.content()
    );

    // The adoption menu offers the starters by their Turkish names.
    let menu = sim.command(user, "pet", pet_command("adopt")).await;
    assert_eq!(menu.title(), "Bir hayvan sahiplen");
    let starter = content.pets.iter().find(|p| p.starter).expect("a starter pet");
    let labels: Vec<_> = menu.options("pa").iter().map(|o| o.label.clone()).collect();
    assert!(
        labels.contains(&content.name(&starter.id, Lang::Tr).into_owned()),
        "{labels:?}"
    );
    let adopted = sim.pick(user, &menu, "pa", &starter.id).await;
    assert!(
        adopted.description().contains("Eve hoş geldin"),
        "{}",
        adopted.description()
    );
    assert!(
        adopted
            .followups
            .iter()
            .any(|f| f.title().starts_with("Rozet açıldı: ")),
        "the first badge is announced in Turkish"
    );
    keep_png(&sim, &adopted, "pet-card-awake");
    for label in ["Besle (20 kabuk)", "Oyna", "Gezdir", "Yıka", "Gezi", "Yuva"] {
        adopted.button(label);
    }

    // Care speaks Turkish.
    let cared = sim.click(user, &adopted, "Oyna").await;
    assert!(
        cared.description().contains("seninle zıplayıp oynuyor. +"),
        "{}",
        cared.description()
    );
    let again = sim.click(user, &cared, "Oyna").await;
    assert!(
        again.ephemeral && again.content().contains("biraz dinlenmeli"),
        "{}",
        again.content()
    );

    let renamed = sim
        .command(
            user,
            "pet",
            json!([{ "name": "rename", "type": 1, "options": [text_option("name", "Pinti")] }]),
        )
        .await;
    assert_eq!(renamed.content(), "Hayvanının yeni adı **Pinti**.");

    // A trip: the picker lists the places and says what the locked ones need.
    let card = sim.command(user, "pet", pet_command("view")).await;
    assert!(card.title().starts_with("Pinti ("), "{}", card.title());
    let picker = sim.click(user, &card, "Gezi").await;
    assert_eq!(picker.title(), "Pinti nereye gitsin?");
    assert!(picker.description().contains("🔒 **"), "{}", picker.description());
    assert!(
        picker.description().contains("seviyede açılır"),
        "{}",
        picker.description()
    );
    keep_png(&sim, &picker, "pet-trip-picker");
    let gone = sim.pick(user, &picker, "py:s:", "tidepool_meadow").await;
    assert!(
        gone.description().contains("için yola çıktı. 1 sa sonra dönecek."),
        "{}",
        gone.description()
    );
    keep_png(&sim, &gone, "pet-card-away");
    assert!(
        gone.controls().any(|c| matches!(
            c,
            Component::Button(b) if b.disabled && b.label.as_deref().is_some_and(|l| l.ends_with("sonra döner"))
        )),
        "the countdown button reads in Turkish"
    );
    let pet_id = db.pets(user).await.unwrap()[0].id;
    let busy = sim.component(user, &format!("pc:walk:{pet_id}"), &[], &gone).await;
    assert!(
        busy.ephemeral && busy.content().contains("gezisinde"),
        "{}",
        busy.content()
    );
    let early = sim.command(user, "pet", trip_claim()).await;
    assert!(early.content().contains("sonra dönecek"), "{}", early.content());

    // An hour on it is home with a souvenir.
    app.shift_clock(chrono::Duration::minutes(61));
    let back = sim.command(user, "pet", pet_command("view")).await;
    assert!(
        back.description().contains("gezisinden bir hatırayla döndü"),
        "{}",
        back.description()
    );
    let claimed = sim.click(user, &back, "Hatırayı al").await;
    assert!(claimed.description().contains("kabuk"), "{}", claimed.description());
    assert!(claimed.description().contains("**+"), "{}", claimed.description());
    keep_png(&sim, &claimed, "pet-card-claimed");

    // The nursery and the capsule.
    let nursery = sim.command(user, "pet", pet_command("list")).await;
    assert_eq!(nursery.title(), "Yuvan");
    assert!(
        nursery.description().contains("**1** hayvan"),
        "{}",
        nursery.description()
    );
    keep_png(&sim, &nursery, "nursery");
    let poor = sim.command(user, "pet", pet_command("capsule")).await;
    assert!(
        poor.ephemeral && poor.content().contains("Bir kapsül 1,500 kabuk") && poor.content().contains("Şanslar: "),
        "{}",
        poor.content()
    );
    fund(&sim, user, 5_000).await;
    let capsule = sim.command(user, "pet", pet_command("capsule")).await;
    assert!(
        capsule.description().contains("Kapsül çatlayıp açılıyor") || capsule.description().contains("zaten sende var"),
        "{}",
        capsule.description()
    );

    // The same pet in English still says what it always said.
    sim.locale = "en-US";
    let english = sim.command(user, "pet", pet_command("view")).await;
    assert!(english.title().starts_with("Pinti the "), "{}", english.title());
    english.button("Nursery");
}

#[tokio::test]
async fn a_sleeping_pet_and_a_busy_nursery_read_in_turkish() {
    let Some(mut sim) = Sim::new("a_sleeping_pet_and_a_busy_nursery_read_in_turkish").await else {
        return;
    };
    let (user, db, app) = (unique(), sim.app.db.clone(), sim.app.clone());
    let content = Content::get();
    sim.locale = "tr";
    let mut pets = Vec::new();
    for (n, name) in ["Pinti", "Tarçın", "Şiş", "Ğıj"].iter().enumerate() {
        pets.push(db.adopt(user, &content.pets[n * 3].id, name).await.unwrap());
    }
    let now = app.now();
    let hour = chrono::Duration::hours(1);
    // One neglected for a day and a half, one away, one back with a souvenir.
    set_pet(
        &db,
        user,
        pets[1].id,
        [0, 0, 100, 0],
        12,
        300,
        now - chrono::Duration::hours(40),
    )
    .await
    .unwrap();
    db.start_trip(user, &pets[2], "kelp_forest_library", fresh_needs(70), now + hour * 3)
        .await
        .unwrap();
    db.start_trip(user, &pets[3], "tidepool_meadow", fresh_needs(70), now - hour)
        .await
        .unwrap();

    let nursery = sim.command(user, "pet", pet_command("list")).await;
    assert!(
        nursery.description().contains("**4** hayvan")
            && nursery.description().contains("1 yolda")
            && nursery.description().contains("1 uyuyor")
            && nursery.description().contains("1 hatırayla döndü"),
        "{}",
        nursery.description()
    );
    assert_eq!(nursery.menu("pn:o:").placeholder.as_deref(), Some("Bir hayvan aç…"));
    let sleepy = nursery
        .options("pn:o:")
        .iter()
        .find(|o| o.label == "Tarçın")
        .expect("the sleeper in the menu");
    assert!(
        sleepy
            .description
            .as_deref()
            .is_some_and(|d| d.starts_with("Sv 12 · ") && d.ends_with("uyuyor")),
        "{:?}",
        sleepy.description
    );
    keep_png(&sim, &nursery, "nursery-busy");

    // Opening the sleeper shows the Turkish card, and waking it is free once a day.
    let card = sim.pick(user, &nursery, "pn:o:", &pets[1].id.to_string()).await;
    assert!(card.description().contains("uyuyakaldı"), "{}", card.description());
    keep_png(&sim, &card, "pet-card-asleep");
    assert!(card.button("Oyna").disabled && card.button("Besle (20 kabuk)").disabled);
    let woke = sim.click(user, &card, "Uyandır (bugün ücretsiz)").await;
    assert!(
        woke.description().contains("bugünkü ücretsiz uyandırma hakkındı"),
        "{}",
        woke.description()
    );
    // Somebody else's click on the pet is refused.
    let theirs = sim
        .component(unique(), &format!("pc:play:{}", pets[0].id), &[], &woke)
        .await;
    assert!(
        theirs.ephemeral && theirs.content().contains("Bu senin hayvanın değil"),
        "{}",
        theirs.content()
    );
}

#[tokio::test]
async fn the_card_editors_speak_turkish() {
    let Some(mut sim) = Sim::new("the_card_editors_speak_turkish").await else {
        return;
    };
    let (user, db) = (unique(), sim.app.db.clone());
    let content = Content::get();
    sim.locale = "tr";
    db.add_message_xp(sim.guild, user, 1_200, 0).await.unwrap();
    db.grant_title(user, "tide_angler").await.unwrap();
    let squid = &content.fish[0];
    db.log_catch(user, &squid.id, 31.5).await.unwrap();
    let pet = db.adopt(user, &content.pets[0].id, "Pinti").await.unwrap();

    // Titles.
    let titles = sim
        .command(user, "customize", json!([{ "name": "title", "type": 1 }]))
        .await;
    assert_eq!(titles.title(), "Unvanların");
    assert!(
        titles.description().contains("Henüz bir unvanın yok.") && titles.description().contains("kilidini açtın"),
        "{}",
        titles.description()
    );
    assert!(titles.button("Çıkar").disabled);
    assert!(
        titles.fields_starting("Kazanılacaklar").contains("**"),
        "the titles still to earn are listed"
    );
    assert!(
        !titles.fields_starting("Açılanlar").is_empty(),
        "the granted title is among the unlocked"
    );
    let picked = sim.pick(user, &titles, "ti:s:", "tide_angler").await;
    assert!(
        picked.description().contains("Kaydedildi. **") && picked.description().contains("adının altında"),
        "{}",
        picked.description()
    );
    keep_png(&sim, &picked, "titles");
    let removed = sim.click(user, &picked, "Çıkar").await;
    assert!(
        removed
            .description()
            .contains("Çıkarıldı. Adın yine tek başına duruyor.")
    );

    // Style.
    let style = sim
        .command(user, "customize", json!([{ "name": "style", "type": 1 }]))
        .await;
    assert_eq!(style.title(), "Kart stili");
    assert!(
        style.description().contains("**Vurgu** Otomatik (mercan)") && style.description().contains("**Çerçeve** Sade"),
        "{}",
        style.description()
    );
    assert!(style.button("Stili sıfırla").disabled);
    let frames = style.options("cs:f");
    let frame = |id: &str| frames.iter().find(|o| o.value == id).expect("a frame");
    assert_eq!(frame("plain").description.as_deref(), Some("Şu an kartlarında"));
    assert_eq!(frame("rope").description.as_deref(), Some("Ücretsiz"));
    assert!(
        frame("pearl")
            .description
            .as_deref()
            .is_some_and(|d| d.starts_with("Kilitli · 20. seviyeye ulaş")),
        "{:?}",
        frame("pearl").description
    );
    assert_eq!(frame("pearl").label, "İnci");
    assert_eq!(style.options("cs:a")[1].label, "Mercan");
    let accent = sim.pick(user, &style, "cs:a", "3a9bdc").await;
    assert!(
        accent.description().contains("Kaydedildi. Vurgu rengin `#3a9bdc`."),
        "{}",
        accent.description()
    );
    keep_png(&sim, &accent, "style-ocean");
    let form = sim.click(user, &accent, "Hex kodu…").await;
    assert_eq!(form.modal.as_ref().map(|m| m.1.as_str()), Some("Vurgu rengi"));
    let refused = sim.submit(user, &form, "zzz", &accent).await;
    assert!(
        refused.description().contains("Bu bir renk kodu değil"),
        "{}",
        refused.description()
    );
    let reset = sim.click(user, &accent, "Stili sıfırla").await;
    assert!(
        reset.description().contains("Tamam. Kartların"),
        "{}",
        reset.description()
    );

    // Showcase.
    let showcase = sim
        .command(user, "customize", json!([{ "name": "showcase", "type": 1 }]))
        .await;
    assert_eq!(showcase.title(), "Profil vitrini");
    assert!(
        showcase.description().contains("Vitrinin boş") && showcase.description().contains("**Balık** -"),
        "{}",
        showcase.description()
    );
    assert!(showcase.button("Alanı temizle").disabled && showcase.button("Hepsini temizle").disabled);
    assert_eq!(showcase.menu("sw:k").placeholder.as_deref(), Some("Düzenlenecek alan…"));
    let slots: Vec<_> = showcase.options("sw:k").iter().map(|o| o.label.as_str()).collect();
    assert_eq!(slots, ["Favori balık", "Hayvan", "Mobilya"]);
    let fish = &showcase.options("sw:s:f")[0];
    assert_eq!(fish.label, content.name(&squid.id, Lang::Tr));
    assert!(
        fish.description
            .as_deref()
            .is_some_and(|d| d.contains("en iyi 31.5 cm") && d.contains("tutuldu")),
        "{:?}",
        fish.description
    );
    let with_fish = sim.pick(user, &showcase, "sw:s:f", &squid.id).await;
    assert!(
        with_fish.description().contains("Kaydedildi.") && with_fish.description().contains("profil kartında."),
        "{}",
        with_fish.description()
    );
    let pets = sim.pick(user, &with_fish, "sw:k", "p").await;
    assert_eq!(pets.menu("sw:s:p").placeholder.as_deref(), Some("Bir hayvan seç…"));
    let with_pet = sim.pick(user, &pets, "sw:s:p", &pet.id.to_string()).await;
    assert!(
        with_pet.description().contains("**Hayvan** Pinti"),
        "{}",
        with_pet.description()
    );
    keep_png(&sim, &with_pet, "showcase");
    let no_house = sim.component(user, "sw:h:p", &[], &with_pet).await;
    assert!(
        no_house.description().contains("Henüz bir evin yok"),
        "{}",
        no_house.description()
    );
    let cleared = sim.click(user, &with_pet, "Hepsini temizle").await;
    assert!(
        cleared
            .description()
            .contains("Temizlendi. Kartın hep olduğu gibi görünüyor."),
        "{}",
        cleared.description()
    );

    // The badge collection reads in Turkish too, and English is untouched.
    let badges = sim
        .command(user, "customize", json!([{ "name": "badges", "type": 1 }]))
        .await;
    let shop = badges.fields_starting("Mağaza");
    assert!(shop.contains("kabuk") || shop.contains("alındı"), "{shop}");
    keep_png(&sim, &badges, "badges");
    sim.locale = "en-US";
    let english = sim
        .command(user, "customize", json!([{ "name": "showcase", "type": 1 }]))
        .await;
    assert_eq!(english.title(), "Profile showcase");
}
