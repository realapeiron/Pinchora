//! Turkish for the remind group, played through the real handlers: `/remind` and the direct messages
//! the ticker sends, `/leaderboard` and `/wrapped`.

use pinchora_core::i18n::Lang;

#[allow(unused_imports)]
use super::*;

#[tokio::test]
async fn reminders_are_set_listed_cancelled_and_delivered_in_turkish() {
    let Some(mut sim) = Sim::new("reminders_are_set_listed_cancelled_and_delivered_in_turkish").await else {
        return;
    };
    let _turn = REMINDERS.lock().await;
    let user = unique();
    DM_OVERRIDES.lock().unwrap().insert(user, false);
    let hours = chrono::Duration::hours;
    reminder_tick(&sim.app).await.unwrap();
    sim.posts();
    sim.locale = "tr";

    // Switched on while the daily is ready; collecting it keeps the language the claim was answered in.
    let on = sim.command(user, "remind", remind_when("daily", true)).await;
    assert!(
        on.ephemeral && on.content().contains("günlük ödülün hazır"),
        "{}",
        on.content()
    );
    assert!(
        on.content().contains("özel mesaj") && on.content().contains("`/remind me when daily off`"),
        "{}",
        on.content()
    );
    let claimed = sim.command(user, "daily", json!([])).await;
    assert!(claimed.title().ends_with("kabuk"), "{}", claimed.title());
    let stored: String = sqlx::query_scalar("SELECT body FROM reminders WHERE user_id = $1 AND kind = 'daily'")
        .bind(user as i64)
        .fetch_one(sim.app.db.pool())
        .await
        .unwrap();
    assert_eq!(stored, "tr", "the language is kept with the message");

    // The ticker has no interaction and the player never picked a language: the stored one is used.
    sim.app.shift_clock(hours(25));
    reminder_tick(&sim.app).await.unwrap();
    let posts = sim.posts();
    let dms = dms_of(&posts, user);
    assert_eq!(dms.len(), 1, "one message when the daily is ready");
    assert_eq!(dms[0].title(), "Günlük ödülün hazır");
    assert!(
        dms[0].description().contains("Kabuklarını `/daily` ile topla.")
            && dms[0].description().contains("`/remind me when daily off`"),
        "{}",
        dms[0].description()
    );

    // A weekly one is scheduled in Turkish too; a language the player picks later wins over it.
    sim.command(user, "remind", remind_when("weekly", true)).await;
    sim.command(user, "weekly", json!([])).await;
    let off = sim.command(user, "remind", remind_when("monthly", false)).await;
    assert!(
        off.content().contains("aylık ödül için artık mesaj göndermeyeceğim"),
        "{}",
        off.content()
    );

    // A note to yourself: units may be Turkish, mentions are left out.
    let set = sim
        .command(user, "remind", remind_in("2sa30dk", "çayı demle <@123456> @everyone"))
        .await;
    assert!(set.ephemeral, "reminders are private");
    assert_eq!(set.title(), "⏰ Hatırlatıcı kuruldu");
    assert!(
        set.description().contains("mesaj göndereceğim:\nçayı demle")
            && !set.description().contains("123456")
            && !set.description().contains("everyone"),
        "{}",
        set.description()
    );
    let footer = set.embeds[0]
        .footer
        .as_ref()
        .map(|f| f.text.as_str())
        .unwrap_or_default();
    assert!(
        footer.contains("/remind cancel veya /remind list ile iptal et"),
        "{footer}"
    );
    for (duration, text) in [("1 saat", "tost"), ("45 dakika", "yumurta"), ("1gün 2 saat", "ekmek")] {
        let more = sim.command(user, "remind", remind_in(duration, text)).await;
        assert_eq!(more.title(), "⏰ Hatırlatıcı kuruldu", "{duration}: {}", more.content());
    }
    assert_eq!(sim.app.db.text_reminders(user).await.unwrap().len(), 4);

    let list = sim.command(user, "remind", json!([sub("list", vec![])])).await;
    assert_eq!(list.title(), "⏰ Hatırlatıcıların");
    assert!(
        list.field("Hatırlatıcılar").contains("çayı demle"),
        "{}",
        list.field("Hatırlatıcılar")
    );
    let notifications = list.field("Bir şey hazır olunca gelen mesajlar");
    assert!(
        notifications.contains("🔔 Günlük ödül")
            && notifications.contains("🔔 Haftalık ödül")
            && notifications.contains("🔕 Aylık ödül")
            && notifications.contains("🔕 Hayvana bakım zamanı")
            && notifications.contains("🔕 Hayvan gezeden döndü"),
        "{notifications}"
    );
    let footer = list.embeds[0]
        .footer
        .as_ref()
        .map(|f| f.text.as_str())
        .unwrap_or_default();
    assert_eq!(footer, "4/25 hatırlatıcı · bildirimleri /remind me when ile değiştir");
    assert_eq!(
        list.menu("rm:c").placeholder.as_deref(),
        Some("Bir hatırlatıcıyı iptal et…")
    );

    // Cancelling: from the menu (a second pick of the same one finds it gone) and by number.
    let value_of = |label: &str| {
        list.options("rm:c")
            .iter()
            .find(|o| o.label == label)
            .expect(label)
            .value
            .clone()
    };
    let (tost, yumurta) = (value_of("tost"), value_of("yumurta"));
    let after = sim.pick(user, &list, "rm:c", &tost).await;
    assert!(
        after.description().contains("numaralı hatırlatıcı iptal edildi"),
        "{}",
        after.description()
    );
    assert_eq!(after.options("rm:c").len(), 3);
    let stale = sim.component(user, "rm:c", &[&tost], &list).await;
    assert_eq!(stale.description(), "O hatırlatıcı zaten yok.");
    let by_number = sim
        .command(
            user,
            "remind",
            json!([sub("cancel", vec![int_option("id", yumurta.parse().unwrap())])]),
        )
        .await;
    assert!(
        by_number.ephemeral && by_number.content() == format!("**#{yumurta}** numaralı hatırlatıcı iptal edildi."),
        "{}",
        by_number.content()
    );
    let none = sim
        .command(
            user,
            "remind",
            json!([sub("cancel", vec![int_option("id", 987_654_321)])]),
        )
        .await;
    assert!(
        none.content()
            .contains("**#987654321** numaralı bir hatırlatıcın yok. Numaralarını `/remind list` ile görebilirsin."),
        "{}",
        none.content()
    );

    // What cannot be read, in Turkish; the units it names are the ones it accepts.
    for (duration, text, answer) in [
        ("yakında", "x", "Bu süreyi okuyamadım"),
        ("30sn", "x", "Bu çok yakın"),
        ("400g", "x", "bir yıl"),
        ("5dk", "<@1> @here", "Neyi hatırlatmam gerektiğini yaz"),
    ] {
        let refused = sim.command(user, "remind", remind_in(duration, text)).await;
        assert!(
            refused.ephemeral && refused.content().contains(answer),
            "{duration}: {}",
            refused.content()
        );
    }
    let bad = sim.command(user, "remind", remind_in("yakında", "x")).await;
    assert!(
        bad.content().contains("(birimler: hf, g, sa, dk, sn)"),
        "{}",
        bad.content()
    );

    // A note arrives in the language the player picked (a text reminder knows no other).
    sim.command(user, "remind", remind_in("5dk", "su iç")).await;
    sim.app.set_user_lang(user, Some(Lang::Tr)).await.unwrap();
    sim.app.shift_clock(chrono::Duration::minutes(6));
    reminder_tick(&sim.app).await.unwrap();
    let posts = sim.posts();
    let dms = dms_of(&posts, user);
    assert_eq!(dms.len(), 1, "{:?}", dms.iter().map(|d| d.title()).collect::<Vec<_>>());
    assert_eq!((dms[0].title(), dms[0].description()), ("⏰ Hatırlatıcı", "su iç"));

    // The player's own pick beats the language kept with the message: English from here on.
    sim.app.set_user_lang(user, Some(Lang::En)).await.unwrap();
    sim.app.shift_clock(chrono::Duration::days(8));
    reminder_tick(&sim.app).await.unwrap();
    let posts = sim.posts();
    let weekly: Vec<_> = dms_of(&posts, user)
        .into_iter()
        .filter(|d| d.title().contains("weekly"))
        .collect();
    assert_eq!(weekly.len(), 1, "the weekly message");
    assert_eq!(weekly[0].title(), "Your weekly is ready");
    assert!(
        weekly[0].description().contains("Collect your shells with `/weekly`."),
        "{}",
        weekly[0].description()
    );
    let english = sim.command(user, "remind", json!([sub("list", vec![])])).await;
    assert_eq!(english.title(), "⏰ Your reminders");
}

#[tokio::test]
async fn pet_and_trip_messages_come_in_turkish() {
    let Some(mut sim) = Sim::new("pet_and_trip_messages_come_in_turkish").await else {
        return;
    };
    let _turn = REMINDERS.lock().await;
    let (user, db) = (unique(), sim.app.db.clone());
    DM_OVERRIDES.lock().unwrap().insert(user, false);
    reminder_tick(&sim.app).await.unwrap();
    sim.posts();
    sim.locale = "tr";
    let content = Content::get();
    let pet = db.adopt(user, &content.pets[0].id, "Pinchy").await.unwrap();

    let on = sim.command(user, "remind", remind_when("pet", true)).await;
    assert!(
        on.content()
            .contains("**Pinchy** bakıma ihtiyaç duyduğunda sana mesaj göndereceğim, yaklaşık <t:"),
        "{}",
        on.content()
    );
    let none = sim.command(user, "remind", remind_when("trip", true)).await;
    assert!(
        none.content().contains("Hiçbir hayvanın gezide değil"),
        "{}",
        none.content()
    );

    // Off on a trip: told when the pet is home, with a souvenir.
    sim.command(user, "pet", trip_go("tidepool_meadow", None)).await;
    let home_at = db
        .scheduled_reminder(user, "trip", pet.id)
        .await
        .unwrap()
        .expect("trip scheduled");
    let away = sim.command(user, "remind", remind_when("trip", true)).await;
    assert!(
        away.content()
            .contains("**Pinchy** gezide. Eve dönünce sana mesaj göndereceğim."),
        "{}",
        away.content()
    );
    set_clock(&sim, home_at + chrono::Duration::minutes(1));
    reminder_tick(&sim.app).await.unwrap();
    let posts = sim.posts();
    let dms = dms_of(&posts, user);
    assert_eq!(dms.len(), 1, "told when the pet is home");
    assert_eq!(dms[0].title(), "Pinchy eve döndü");
    assert!(
        dms[0].description().contains("Hatırayı `/pet trip claim` ile al.")
            && dms[0].description().contains("`/remind me when trip off`"),
        "{}",
        dms[0].description()
    );

    // Claimed: its next message is about what it wants most (the fun runs low first).
    sim.command(
        user,
        "pet",
        json!([{ "name": "trip", "type": 2, "options": [{ "name": "claim", "type": 1 }] }]),
    )
    .await;
    let due = db
        .scheduled_reminder(user, "pet", pet.id)
        .await
        .unwrap()
        .expect("the pet is watched again");
    set_clock(&sim, due + chrono::Duration::minutes(1));
    reminder_tick(&sim.app).await.unwrap();
    let posts = sim.posts();
    let dms = dms_of(&posts, user);
    assert_eq!(dms.len(), 1, "told when the pet needs looking after");
    assert_eq!(dms[0].title(), "Pinchy sana ihtiyaç duyuyor");
    assert!(
        dms[0].description().contains("Ona biraz oyun iyi gelir.")
            && dms[0].description().contains("`/pet view`")
            && dms[0].description().contains("`/remind me when pet off`"),
        "{}",
        dms[0].description()
    );
}

#[tokio::test]
async fn the_leaderboards_are_drawn_in_turkish() {
    let Some(mut sim) = Sim::new("the_leaderboards_are_drawn_in_turkish").await else {
        return;
    };
    let week = pinchora_core::leveling::week_start(sim.app.now());
    let last = week - chrono::Duration::weeks(1);
    let (viewer, alice, bob, carol) = (unique(), unique(), unique(), unique());
    sim.locale = "tr";

    // Nobody here yet: the empty texts, and the page buttons.
    for board in ["server", "catch"] {
        let empty = sim
            .command(viewer, "leaderboard", json!([text_option("board", board)]))
            .await;
        assert_eq!(png_height(&empty), 716, "{board}");
        assert!(empty.button("◀").disabled && empty.button("▶").disabled);
    }

    let grant = |xp, points| pinchora_db::XpGrant {
        server_xp: xp,
        global_xp: xp,
        points,
    };
    for (user, xp, points) in [(alice, 4_200, 90), (bob, 1_300, 40), (carol, 600, 5), (viewer, 80, 1)] {
        sim.app
            .db
            .add_xp(sim.guild, user, grant(xp, points), week)
            .await
            .unwrap();
        fund(&sim, user, xp * 3).await;
    }
    for (user, xp) in [(bob, 5_200), (alice, 2_000), (carol, 90)] {
        sim.app.db.add_xp(sim.guild, user, grant(xp, 0), last).await.unwrap();
    }
    for (user, fish, size) in [
        (alice, "pearlpatch_koi", 66.0),
        (bob, "reefruff_lionfish", 36.0),
        (carol, "coralchorus_sailfish", 200.0),
    ] {
        catch_for_board(&sim, user, fish, size, week).await;
    }
    for (user, fish, size) in [(bob, "moonbutton_sunfish", 80.0), (alice, "petalveil_betta", 8.0)] {
        catch_for_board(&sim, user, fish, size, last).await;
    }

    // Every board, English first and then Turkish: the pictures differ and keep their size.
    let boards: [(&str, u32); 6] = [
        ("server", 716),
        ("points", 716),
        ("weekly", 826),
        ("catch", 826),
        ("xp", 716),
        ("shells", 716),
    ];
    for (board, height) in boards {
        sim.locale = "en-US";
        let english = sim
            .command(viewer, "leaderboard", json!([text_option("board", board)]))
            .await;
        sim.locale = "tr";
        let turkish = sim
            .command(viewer, "leaderboard", json!([text_option("board", board)]))
            .await;
        assert_eq!(png_height(&english), height, "{board}");
        assert_eq!(png_height(&turkish), height, "{board}");
        assert_ne!(english.files[0].1, turkish.files[0].1, "{board} is drawn in Turkish");
        assert_eq!(turkish.files[0].0, "leaderboard.png");
        let id = turkish.button("◀").custom_id.clone().unwrap();
        assert_eq!(id, format!("lb:{board}:0"), "its buttons keep the {board} board");
        let again = sim.component(viewer, &id, &[], &turkish).await;
        assert_eq!(png_height(&again), height, "{board} again");
    }
}

#[tokio::test]
async fn wrapped_is_told_and_drawn_in_turkish() {
    let Some(mut sim) = Sim::new("wrapped_is_told_and_drawn_in_turkish").await else {
        return;
    };
    let (user, host, newcomer, db) = (unique(), unique(), unique(), sim.app.db.clone());
    let year = Utc::now().year();
    let at = |y: i32, m: u32, d: u32| Utc.with_ymd_and_hms(y, m, d, 12, 0, 0).unwrap();
    sim.locale = "tr";

    let counted = [
        (
            Counter::Messages,
            [(1, 120), (2, 300), (5, 80), (8, 560), (9, 410), (11, 980)],
        ),
        (
            Counter::Xp,
            [(1, 2_400), (2, 6_100), (5, 1_500), (8, 7_200), (9, 8_300), (11, 19_500)],
        ),
        (
            Counter::Shells,
            [(1, 900), (2, 4_000), (5, 700), (8, 9_000), (9, 5_500), (11, 12_000)],
        ),
        (Counter::PetCare, [(1, 10), (2, 30), (5, 8), (8, 50), (9, 40), (11, 90)]),
        (Counter::LikesGiven, [(1, 1), (2, 4), (5, 0), (8, 6), (9, 3), (11, 9)]),
        (
            Counter::LikesReceived,
            [(1, 2), (2, 3), (5, 1), (8, 8), (9, 2), (11, 12)],
        ),
    ];
    for (counter, months) in counted {
        for (month, n) in months {
            db.add_activity_in(user, NaiveDate::from_ymd_opt(year, month, 1).unwrap(), counter, n)
                .await
                .unwrap();
        }
    }
    let fish = &Content::get().fish[2];
    for size in [10.0, 12.0, 14.0] {
        db.log_catch(user, &fish.id, size).await.unwrap();
    }
    db.befriend(user, host).await.unwrap();
    db.remember_name(host, "Marlowe the Mapmaker").await.unwrap();
    for day in [3, 9, 15] {
        db.record_visit(host, user, NaiveDate::from_ymd_opt(year, 6, day).unwrap(), 0, 10)
            .await
            .unwrap();
    }

    // Closed: when it opens, in Turkish.
    set_clock(&sim, at(year, 6, 15));
    let closed = sim.command(user, "wrapped", json!([])).await;
    let opens = Utc.with_ymd_and_hms(year, 12, 1, 0, 0, 0).unwrap().timestamp();
    assert!(
        closed.ephemeral
            && closed.files.is_empty()
            && closed.content().contains(&format!("<t:{opens}:D> tarihinde açılıyor"))
            && closed.content().contains("saymaya devam ediyor"),
        "{}",
        closed.content()
    );

    // Open: the card of the year, and a quiet one for somebody with nothing.
    set_clock(&sim, Utc.with_ymd_and_hms(year, 12, 1, 0, 0, 5).unwrap());
    let card = sim.command(user, "wrapped", json!([])).await;
    assert_png(&card, 640, 1164);
    assert!(!card.ephemeral, "the card is for sharing");
    sim.locale = "en-US";
    let english = sim.command(user, "wrapped", json!([])).await;
    assert_png(&english, 640, 1164);
    assert_ne!(card.files[0].1, english.files[0].1, "the card is drawn in Turkish");
    sim.locale = "tr";
    let quiet = sim.command(newcomer, "wrapped", json!([])).await;
    assert_png(&quiet, 640, 1164);

    // Years: one with nothing, one that is not over.
    let early = sim
        .command(user, "wrapped", json!([int_option("year", (year - 1).into())]))
        .await;
    assert!(
        early.ephemeral && early.content() == format!("{} yılında senin için hiçbir şey sayılmadı.", year - 1),
        "{}",
        early.content()
    );
    let later = sim
        .command(user, "wrapped", json!([int_option("year", (year + 1).into())]))
        .await;
    assert!(
        later.ephemeral
            && later.content()
                == format!(
                    "{} yılı henüz özetlenmedi. En yeni Wrapped {year} yılına ait.",
                    year + 1
                ),
        "{}",
        later.content()
    );
}
