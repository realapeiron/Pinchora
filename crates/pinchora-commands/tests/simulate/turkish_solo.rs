//! The solo puzzle games and the collection games in Turkish: Pixel Picross, Tide 2048, critter drops and
//! bottles, the dex, beachcomb and the crab pot, and the trivia, hangman and wordle paths `turkish.rs` leaves
//! out. Pictures land in `target/sim/<test>/`.

use chrono::Duration;
use pinchora_commands::{collection_tick, post_bottle, post_critter_drop};
use pinchora_core::i18n::Lang;
use pinchora_game::{
    collection::Collection,
    lang,
    play::{
        find_question,
        picross::{self, Puzzle},
        tide2048,
    },
};

use super::{collection::COLLECTION, *};

/// Keeps the picture of a reply next to the sim's own, under a name that says what it shows.
fn keep_png(sim: &Sim, reply: &Reply, name: &str) {
    let (_, png) = reply.files.first().expect("the reply carried a picture");
    std::fs::write(sim.dir.join(format!("{name}.png")), png).expect("save the picture");
}

/// How Discord writes a mention.
fn mention(user: u64) -> String {
    format!("<@{user}>")
}

/// The row menu values that fill row `y` of the puzzle's finished picture.
fn row_values(puzzle: &Puzzle, y: usize) -> Vec<String> {
    (0..picross::SIZE)
        .filter(|&x| puzzle.filled(y * picross::SIZE + x))
        .map(|x| x.to_string())
        .collect()
}

/// Puts entries in a player's dex without playing for them.
async fn give(sim: &Sim, user: u64, entries: &[&str], caught: i32) {
    for id in entries {
        sqlx::query("INSERT INTO dex_entries (user_id, entry_id, caught) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
            .bind(user as i64)
            .bind(*id)
            .bind(caught)
            .execute(sim.app.db.pool())
            .await
            .expect("dex row");
    }
}

// /Pixel Picross

#[tokio::test]
async fn turkish_picross_is_marked_checked_solved_and_shared() {
    let Some(mut sim) = Sim::new("turkish_picross_is_marked_checked_solved_and_shared").await else {
        return;
    };
    sim.locale = "tr";
    let user = unique();
    let day = sim.app.now().timestamp().div_euclid(86_400);
    let (picture, variant) = picross::daily(day);
    let puzzle = Puzzle::new(picture, variant);

    let board = sim.command(user, "play", json!([sub("picross", vec![])])).await;
    assert!(board.ephemeral);
    assert!(board.title().starts_with("Günlük picross #"), "{}", board.title());
    assert!(
        board.description().contains("**Hücreleri işaretle**") && board.description().contains("**Kontrol et**"),
        "{}",
        board.description()
    );
    assert_eq!(
        board.menu("pz:r:").placeholder.as_deref(),
        Some("Doldurulacak satırı seç")
    );
    assert_eq!(board.options("pz:r:")[0].label, "Satır 1");
    assert!(board.has_button("Hücreleri işaretle") && board.has_button("Kontrol et (3 hak kaldı)"));
    keep_png(&sim, &board, "picross-tr-empty");

    // A row's menu, the form, and a form the handler cannot read.
    let row = sim.pick(user, &board, "pz:r:", "3").await;
    assert_eq!(
        row.menu("pz:c:").placeholder.as_deref(),
        Some("4. satırdaki dolu hücreler")
    );
    let modal = sim.click(user, &row, "Hücreleri işaretle").await;
    assert_eq!(
        modal.modal.as_ref().map(|m| m.1.as_str()),
        Some("Hücreleri işaretle"),
        "{modal}"
    );
    let bad = sim.submit_fields(user, &modal, &[("cells", "A1-B2")]).await;
    assert!(
        bad.ephemeral && bad.content().contains("okuyamadım"),
        "{}",
        bad.content()
    );

    // One wrong mark, a check that counts it, then the mark is cleared again.
    let empty = (0..100).find(|&c| !puzzle.filled(c)).expect("an empty cell");
    let name = format!("{}{}", char::from(b'A' + (empty % 10) as u8), empty / 10 + 1);
    let marked = sim.submit_fields(user, &modal, &[("cells", &name)]).await;
    let checked = sim.click(user, &marked, "Kontrol et (3 hak kaldı)").await;
    assert!(
        checked
            .description()
            .contains("İşaretlerinden **1** tanesi resmin parçası değil. 2 kontrol hakkın kaldı."),
        "{}",
        checked.description()
    );
    keep_png(&sim, &checked, "picross-tr-checked");
    let mut state = sim.submit_fields(user, &modal, &[("cells", &format!("-{name}"))]).await;

    // Every row by the menus: a checked puzzle pays less.
    for y in 0..picross::SIZE {
        let values = row_values(&puzzle, y);
        if values.is_empty() {
            continue;
        }
        let picked = sim.pick(user, &state, "pz:r:", &y.to_string()).await;
        let refs: Vec<&str> = values.iter().map(String::as_str).collect();
        state = sim
            .component(user, &picked.menu("pz:c:").custom_id.clone(), &refs, &picked)
            .await;
    }
    assert!(
        state
            .description()
            .contains(&format!("Buldun: **{}**!", puzzle.name_in(Lang::Tr))),
        "{}",
        state.description()
    );
    assert!(
        state
            .description()
            .contains("+**15** kabuk (kontrol ettiğin için daha az), **1** günlük seri!"),
        "{}",
        state.description()
    );
    assert!(state.has_button("Sonucu paylaş"));
    keep_png(&sim, &state, "picross-tr-solved");
    assert_eq!(shells(&sim, user).await, 15);

    // Sharing posts no picture and only once; a late click on a finished board is told so.
    let shared = sim.click(user, &state, "Sonucu paylaş").await;
    assert!(
        !shared.ephemeral
            && shared.content().contains("günlük Piksel Picross #")
            && shared.content().contains("1 günlük seri"),
        "{}",
        shared.content()
    );
    let twice = sim.click(user, &state, "Sonucu paylaş").await;
    assert!(
        twice.ephemeral && twice.content().contains("zaten paylaştın"),
        "{}",
        twice.content()
    );
    let late = sim
        .component(user, &format!("pz:c:daily:{day}:3"), &["0"], &state)
        .await;
    assert!(
        late.ephemeral && late.content().contains("Bu bulmaca bitti"),
        "{}",
        late.content()
    );

    // Practice puzzles are Turkish too.
    let practice = sim
        .command(
            user,
            "play",
            json!([sub("picross", vec![text_option("mode", "practice")])]),
        )
        .await;
    assert_eq!(practice.title(), "Serbest picross");
    keep_png(&sim, &practice, "picross-tr-practice");
    let stranger = sim.component(unique(), &format!("pz:k:daily:{day}"), &[], &board).await;
    assert!(
        stranger.ephemeral && stranger.content().contains("senin değil"),
        "{}",
        stranger.content()
    );
}

// /Tide 2048

#[tokio::test]
async fn turkish_tide_2048_slides_refuses_and_ends_with_the_creature_names() {
    let Some(mut sim) = Sim::new("turkish_tide_2048_slides_refuses_and_ends_with_the_creature_names").await else {
        return;
    };
    sim.locale = "tr";
    let user = unique();
    let board = sim.command(user, "play", json!([sub("2048", vec![])])).await;
    assert_eq!(board.title(), "Gelgit 2048");
    assert!(
        board.description().contains("Oklarla kaydır") && board.description().contains("**Leviathan** (2048)"),
        "{}",
        board.description()
    );
    assert!(board.has_button("Bitir") && board.has_button("←"));
    keep_png(&sim, &board, "tide2048-tr-start");

    // Ending needs a few moves, and a slide that moves nothing says so.
    let finish = board.button("Bitir").custom_id.clone().expect("id");
    let early = sim.component(user, &finish, &[], &board).await;
    assert!(
        early.ephemeral && early.content().contains("birkaç hamle daha"),
        "{}",
        early.content()
    );
    let moves = ["←", "↑", "↓", "→"];
    if let Some(stuck) = moves.into_iter().find(|label| board.button(label).disabled) {
        let id = board.button(stuck).custom_id.clone().expect("id");
        let nothing = sim.component(user, &id, &[], &board).await;
        assert!(
            nothing.ephemeral && nothing.content().contains("hiçbir şey kaymıyor"),
            "{}",
            nothing.content()
        );
    }
    let arrow = moves
        .into_iter()
        .find(|label| !board.button(label).disabled)
        .expect("some move is possible");
    let moved = sim.click(user, &board, arrow).await;
    assert!(
        moved.description().contains("Oklarla kaydır"),
        "{}",
        moved.description()
    );
    keep_png(&sim, &moved, "tide2048-tr-moved");

    // A board one slide from the end: a latin square of 8, 16 and 512 with one hole.
    let values = [8u32, 16, 512];
    let mut board16 = tide2048::Board::new();
    for cell in 0..15usize {
        let (r, c) = (cell / 4, cell % 4);
        board16.set(cell, values[(c + 2 * r) % 3].trailing_zeros() as u8);
    }
    sqlx::query("UPDATE solo_games SET state = $2, score = 1234, moves = 40 WHERE user_id = $1 AND game = 'tide2048'")
        .bind(user as i64)
        .bind(board16.encode())
        .execute(sim.app.db.pool())
        .await
        .expect("set the board");
    let live = sim.command(user, "play", json!([sub("2048", vec![])])).await;
    let over = sim.click(user, &live, "→").await;
    assert!(
        over.description().contains("Son skor **1234**") && over.description().contains("en büyük karo **512**"),
        "{}",
        over.description()
    );
    assert!(over.description().contains("+**10** kabuk"), "{}", over.description());
    assert!(over.has_button("Yeni oyun") && !over.has_button("Bitir"));
    keep_png(&sim, &over, "tide2048-tr-over");
    assert_eq!(shells(&sim, user).await, 10);

    let fresh = sim.click(user, &over, "Yeni oyun").await;
    assert!(fresh.has_button("Bitir"), "{}", fresh.description());
    let none = sim.component(unique(), "tz:m:l:0", &[], &fresh).await;
    assert!(
        none.ephemeral && none.content().contains("Oyunun yok"),
        "{}",
        none.content()
    );
}

// /Critter drop and the bottle

#[tokio::test]
async fn turkish_critter_drops_and_bottles_wash_up_get_taken_and_drift_away() {
    let Some(mut sim) = Sim::new("turkish_critter_drops_and_bottles_wash_up_get_taken_and_drift_away").await else {
        return;
    };
    let _turn = COLLECTION.lock().await;
    sim.channel = unique();
    let channel = sim.channel;
    let (a, b) = (unique(), unique());
    // What washes up by itself speaks the server's language; the clicks are answered in the player's.
    sim.app
        .set_guild_lang(sim.guild, Some(Lang::Tr))
        .await
        .expect("language");
    sim.locale = "tr";

    assert!(post_critter_drop(&sim.app, sim.guild, channel).await.expect("posts"));
    let posts = sim.posts();
    let post = only(&posts, channel, Method::POST);
    assert_eq!(post.title(), "🦀 Kıyıya yaratıklar vurdu!");
    assert!(
        post.description().contains("kart numarasına bas"),
        "{}",
        post.description()
    );
    let drop = reply_of(post);
    let ana = sim.click(a, &drop, "1").await;
    assert!(
        ana.description().contains(&format!("{} aldı", mention(a))),
        "{}",
        ana.description()
    );
    let details = &ana.followups[0];
    assert!(details.ephemeral);
    assert!(
        details.description().contains("adlı yaratığı aldın") && details.description().contains("defterin için yeni!"),
        "{}",
        details.description()
    );
    keep_png(&sim, details, "critter-tr-took");
    let slow = sim.click(b, &drop, "1").await;
    assert!(
        slow.ephemeral && slow.content().contains("Geç kaldın!"),
        "{}",
        slow.content()
    );
    let again = sim.click(a, &drop, "2").await;
    assert!(
        again.ephemeral && again.content().contains("zaten aldın"),
        "{}",
        again.content()
    );
    sim.app.shift_clock(Duration::seconds(100));
    collection_tick(&sim.app).await.expect("tick");
    let posts = sim.posts();
    let over = posts
        .iter()
        .find(|p| p.message == post.message && p.method == Method::PATCH)
        .expect("the drop's message is edited");
    assert_eq!(over.title(), "🦀 Yaratık etkinliği bitti");
    assert!(
        over.description().contains("denize karıştı") && !over.description().contains("boşta"),
        "{}",
        over.description()
    );

    // A bottle: the channel reads it in the server's Turkish even when an English player opens it, a late one
    // is told so, an unopened one drifts away.
    assert!(post_bottle(&sim.app, sim.guild, channel).await.expect("posts"));
    let posts = sim.posts();
    let post = only(&posts, channel, Method::POST);
    assert_eq!(post.title(), "🍾 Kıyıya bir şişe vurdu!");
    let bottle = reply_of(post);
    assert!(bottle.has_button("Şişeyi aç"));
    sim.locale = "en-US";
    let opened = sim.click(a, &bottle, "Şişeyi aç").await;
    sim.locale = "tr";
    assert_eq!(opened.title(), "🍾 Şişe açıldı");
    assert!(
        opened.description().contains(&format!("Şişeyi açan: {}", mention(a))),
        "{}",
        opened.description()
    );
    assert!(
        opened.description().contains("İçinde:") || opened.description().contains("döküldü"),
        "{}",
        opened.description()
    );
    assert!(opened.button("Açıldı").disabled);
    let slow = sim.click(b, &bottle, "Şişeyi aç").await;
    assert!(
        slow.ephemeral && slow.content().contains(&format!("Şişeyi önce {} açtı", mention(a))),
        "{}",
        slow.content()
    );
    let twice = sim.click(a, &bottle, "Şişeyi aç").await;
    assert!(
        twice.ephemeral && twice.content().contains("zaten sen açtın"),
        "{}",
        twice.content()
    );
    assert!(post_bottle(&sim.app, sim.guild, channel).await.expect("posts"));
    let posts = sim.posts();
    let second_post = only(&posts, channel, Method::POST);
    let second = second_post.message;
    let second_open = reply_of(second_post)
        .button("Şişeyi aç")
        .custom_id
        .clone()
        .expect("custom id");
    sim.app.shift_clock(Duration::minutes(11));
    collection_tick(&sim.app).await.expect("tick");
    let posts = sim.posts();
    let gone = posts
        .iter()
        .find(|p| p.message == second && p.method == Method::PATCH)
        .expect("the bottle's message is edited");
    assert_eq!(gone.title(), "🍾 Şişe denize karıştı");
    let adrift = reply_of(gone);
    assert!(adrift.button("Denize karıştı").disabled);
    let late = sim.component(b, &second_open, &[], &adrift).await;
    assert!(
        late.ephemeral && late.content().contains("uzaklara gitti"),
        "{}",
        late.content()
    );
}

#[tokio::test]
async fn turkish_critterdrop_command_and_settings_answer_in_turkish() {
    let Some(mut sim) = Sim::new("turkish_critterdrop_command_and_settings_answer_in_turkish").await else {
        return;
    };
    let _turn = COLLECTION.lock().await;
    sim.channel = unique();
    sim.locale = "tr";
    sim.permissions = MANAGE_GUILD;
    let (admin, host) = (unique(), unique());
    let elsewhere = unique();

    let off = sim.command(host, "critterdrop", json!([])).await;
    assert!(
        off.ephemeral && off.content().contains("kapalı") && off.content().contains("/config critters"),
        "{}",
        off.content()
    );
    let both = sim
        .command_resolved(
            admin,
            "config",
            json!([sub(
                "critters",
                vec![channel_option("channel", elsewhere), bool_option("anywhere", true)]
            )]),
            resolved_channels(&[elsewhere]),
        )
        .await;
    assert!(both.content().contains("ikisini birden değil"), "{}", both.content());
    let kept = sim
        .command_resolved(
            admin,
            "config",
            json!([sub(
                "critters",
                vec![bool_option("enabled", true), channel_option("channel", elsewhere)]
            )]),
            resolved_channels(&[elsewhere]),
        )
        .await;
    assert!(
        kept.content().contains("**açık**") && kept.content().contains(&format!("<#{elsewhere}>")),
        "{}",
        kept.content()
    );
    let wrong = sim.command(host, "critterdrop", json!([])).await;
    assert!(
        wrong.ephemeral && wrong.content().contains(&format!("<#{elsewhere}>")) && wrong.content().contains("başlat"),
        "{}",
        wrong.content()
    );
    let anywhere = sim
        .command(
            admin,
            "config",
            json!([sub("critters", vec![bool_option("anywhere", true)])]),
        )
        .await;
    assert!(
        anywhere.content().contains("sohbetin yoğun olduğu kanalda"),
        "{}",
        anywhere.content()
    );
    let shown = sim.command(admin, "config", json!([sub("show", vec![])])).await;
    assert!(
        shown
            .field("Yaratık etkinliği")
            .contains("sohbetin yoğun olduğu kanalda"),
        "{}",
        shown.field("Yaratık etkinliği")
    );

    // The drop is posted for the whole channel, so it speaks the server's language.
    sim.app
        .set_guild_lang(sim.guild, Some(Lang::Tr))
        .await
        .expect("language");
    let drop = sim.command(host, "critterdrop", json!([])).await;
    assert!(!drop.ephemeral);
    assert_eq!(drop.title(), "🦀 Kıyıya yaratıklar vurdu!");
    assert!(
        drop.description().contains(&format!("Başlatan: {}", mention(host))),
        "{}",
        drop.description()
    );
    keep_png(&sim, &drop, "drop-tr");
    let busy = sim.command(unique(), "critterdrop", json!([])).await;
    assert!(
        busy.ephemeral && busy.content().contains("açık bir yaratık etkinliği"),
        "{}",
        busy.content()
    );
    sim.app.shift_clock(Duration::seconds(100));
    collection_tick(&sim.app).await.expect("tick");
    let soon = sim.command(host, "critterdrop", json!([])).await;
    assert!(
        soon.ephemeral
            && soon
                .content()
                .contains("Bu kanalda biraz önce yaratık etkinliği yapıldı"),
        "{}",
        soon.content()
    );
    let _ = sim.posts();
}

// /Beachcomb, /dex and /crabpot

#[tokio::test]
async fn turkish_beachcomb_walks_then_waits_for_the_tide() {
    let Some(mut sim) = Sim::new("turkish_beachcomb_walks_then_waits_for_the_tide").await else {
        return;
    };
    sim.locale = "tr";
    let user = unique();
    let walk = sim.command(user, "beachcomb", json!([])).await;
    assert!(walk.ephemeral);
    assert_eq!(walk.files[0].0, "beachcomb.png");
    assert!(walk.description().contains("🆕"), "{}", walk.description());
    let dex = sim.app.db.dex(user).await.unwrap();
    let entry = Collection::get().entry(&dex[0].entry_id).expect("a real entry");
    assert!(
        walk.description().contains(&entry.name_in(Lang::Tr)),
        "{}",
        walk.description()
    );
    assert!(walk.description().contains(" kabuk"), "{}", walk.description());
    keep_png(&sim, &walk, "beachcomb-tr");
    let soon = sim.command(user, "beachcomb", json!([])).await;
    assert!(
        soon.ephemeral && soon.content().contains("Biraz önce sahilde yürüdün"),
        "{}",
        soon.content()
    );
    // A rarer find than usual, and one past the daily limit, for the card's other texts.
    let other = unique();
    for _ in 0..4 {
        sim.app.shift_clock(Duration::minutes(21));
        let walk = sim.command(other, "beachcomb", json!([])).await;
        keep_png(&sim, &walk, "beachcomb-tr-more");
    }
}

#[tokio::test]
async fn turkish_dex_pages_both_tabs_and_trades_fragments() {
    let Some(mut sim) = Sim::new("turkish_dex_pages_both_tabs_and_trades_fragments").await else {
        return;
    };
    sim.locale = "tr";
    let (owner, visitor) = (unique(), unique());
    let collection = Collection::get();
    let critters: Vec<&str> = collection.critters.iter().take(25).map(|e| e.id.as_str()).collect();
    let finds: Vec<&str> = collection.finds.iter().take(9).map(|e| e.id.as_str()).collect();
    give(&sim, owner, &critters, 1).await;
    give(&sim, owner, &finds[..3], 4).await;
    give(&sim, owner, &finds[3..], 1).await;
    sqlx::query(
        "INSERT INTO collection_players (user_id, fragments) VALUES ($1, 120)
         ON CONFLICT (user_id) DO UPDATE SET fragments = 120",
    )
    .bind(owner as i64)
    .execute(sim.app.db.pool())
    .await
    .expect("fragments");

    let book = sim.command(owner, "dex", json!([])).await;
    assert!(
        book.description().contains("Gelgit Havuzu Defteri")
            && book.description().contains("yaratıklar 25/48")
            && book.description().contains("34 / 84 bulundu"),
        "{}",
        book.description()
    );
    assert!(
        book.button("Yaratıklar").disabled && !book.button("Buluntular").disabled,
        "tabs"
    );
    assert!(book.has_button("Parçacıkları harca"));
    keep_png(&sim, &book, "dex-tr-critters");
    let second = sim.click(owner, &book, "▶").await;
    keep_png(&sim, &second, "dex-tr-critters-2");
    let finds_tab = sim.click(owner, &second, "Buluntular").await;
    assert!(
        finds_tab.description().contains("buluntular 9/36"),
        "{}",
        finds_tab.description()
    );
    keep_png(&sim, &finds_tab, "dex-tr-finds");

    // Somebody else's dex has nothing to spend, and cannot spend yours.
    let theirs = sim
        .command_resolved(
            visitor,
            "dex",
            json!([user_option("user", owner), text_option("tab", "finds")]),
            resolved(&[owner], &[]),
        )
        .await;
    assert!(!theirs.has_button("Parçacıkları harca"));
    keep_png(&sim, &theirs, "dex-tr-visitor");
    let sneaky = sim.component(visitor, &format!("bc:x:{owner}"), &[], &theirs).await;
    assert!(
        sneaky.ephemeral && sneaky.content().contains("Bu defter senin değil"),
        "{}",
        sneaky.content()
    );

    let panel = sim.click(owner, &book, "Parçacıkları harca").await;
    assert_eq!(panel.title(), "Parçacık takası");
    assert!(
        panel.description().contains("**120** parçacığın var") && panel.description().contains("ruh bağlıdır"),
        "{}",
        panel.description()
    );
    assert_eq!(panel.options("bc:xs:")[0].label, "Yaygın dekor parçası");
    assert_eq!(
        panel.menu("bc:xs:").placeholder.as_deref(),
        Some("Takas edilecek parçayı seç…")
    );
    let broke = sim.pick(owner, &panel, "bc:xs:", "legendary").await;
    assert!(
        broke
            .description()
            .contains("**8.900** parçacıkla efsanevi bir parça alabilirsin"),
        "{}",
        broke.description()
    );
    let bought = sim.pick(owner, &panel, "bc:xs:", "common").await;
    assert!(bought.title().ends_with(" senin oldu!"), "{}", bought.title());
    assert!(
        bought.description().contains("Bu parça yalnızca senin"),
        "{}",
        bought.description()
    );
    keep_png(&sim, &bought, "dex-tr-piece");
    let back = sim.click(owner, &bought, "Deftere dön").await;
    assert_eq!(back.files[0].0, "dex.png");
}

#[tokio::test]
async fn turkish_crab_pot_soaks_hauls_and_upgrades() {
    let Some(mut sim) = Sim::new("turkish_crab_pot_soaks_hauls_and_upgrades").await else {
        return;
    };
    sim.locale = "tr";
    let user = unique();

    let empty = sim.command(user, "crabpot", json!([])).await;
    assert!(
        empty.description().contains("Yengeç kafesi, seviye 1: boş."),
        "{}",
        empty.description()
    );
    for label in [
        "1 saatliğine kur",
        "4 saatliğine kur",
        "8 saatliğine kur",
        "Yükselt (400 kabuk)",
    ] {
        assert!(empty.has_button(label), "{label}");
    }
    keep_png(&sim, &empty, "pot-tr-empty");
    let soaking = sim.click(user, &empty, "1 saatliğine kur").await;
    assert!(
        soaking.description().contains("Kafes 1 saatliğine suda"),
        "{}",
        soaking.description()
    );
    keep_png(&sim, &soaking, "pot-tr-soaking");
    let twice = sim.click(user, &empty, "8 saatliğine kur").await;
    assert!(
        twice.description().contains("Kafes zaten suda"),
        "{}",
        twice.description()
    );
    let early = sim.click(user, &soaking, "Gelgite bak").await;
    assert!(early.description().contains("suda, "), "{}", early.description());
    let thief = sim.component(unique(), &format!("kp:h:{user}"), &[], &early).await;
    assert!(
        thief.ephemeral && thief.content().contains("Bu kafes senin değil"),
        "{}",
        thief.content()
    );

    sim.app.shift_clock(Duration::minutes(61));
    let ready = sim.command(user, "crabpot", json!([])).await;
    assert!(ready.has_button("Çek!"), "{}", ready.description());
    keep_png(&sim, &ready, "pot-tr-ready");
    let haul = sim.click(user, &ready, "Çek!").await;
    assert!(haul.description().contains("**1** av çektin"), "{}", haul.description());
    keep_png(&sim, &haul, "haul-tr-one");
    let again = sim.click(user, &ready, "Çek!").await;
    assert!(again.description().contains("Kafesin boş"), "{}", again.description());

    let reset = sim.click(user, &haul, "Kafese dön").await;
    let soak = sim.click(user, &reset, "8 saatliğine kur").await;
    assert!(soak.has_button("Gelgite bak"));
    sim.app.shift_clock(Duration::hours(9));
    let ready = sim.command(user, "crabpot", json!([])).await;
    let big = sim.click(user, &ready, "Çek!").await;
    assert!(big.description().contains("**10** av çektin"), "{}", big.description());
    keep_png(&sim, &big, "haul-tr-many");

    // Upgrading costs shells, and says so when there are not enough.
    sqlx::query("UPDATE users SET credits = 0 WHERE id = $1")
        .bind(user as i64)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    let pot = sim.command(user, "crabpot", json!([])).await;
    let poor = sim.click(user, &pot, "Yükselt (400 kabuk)").await;
    assert!(
        poor.description().contains("**400** kabuk tutuyor"),
        "{}",
        poor.description()
    );
    fund(&sim, user, 500).await;
    let upgraded = sim.click(user, &pot, "Yükselt (400 kabuk)").await;
    assert!(
        upgraded.description().contains("seviye 2"),
        "{}",
        upgraded.description()
    );
    assert!(upgraded.has_button("Yükselt (1.000 kabuk)"));
    keep_png(&sim, &upgraded, "pot-tr-upgraded");
    let stale = sim.click(user, &pot, "Yükselt (400 kabuk)").await;
    assert!(
        stale.description().contains("artık o seviyede değil"),
        "{}",
        stale.description()
    );
}

// /Trivia, hangman and wordle

#[tokio::test]
async fn a_turkish_trivia_round_locks_answers_reveals_and_pays_the_right_one() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) = Sim::new("a_turkish_trivia_round_locks_answers_reveals_and_pays_the_right_one").await else {
        return;
    };
    forget_late_games(&sim).await;
    sim.channel = unique();
    sim.locale = "tr";
    let (starter, right, wrong, late) = (unique(), unique(), unique(), unique());
    let round = sim
        .command(
            starter,
            "play",
            json!([sub("trivia", vec![text_option("category", "ocean")])]),
        )
        .await;
    let id = session_of(&round);
    let state = sim.app.db.game(id).await.unwrap().unwrap().state;
    let (question_id, order) = state.split_once(':').expect("question:order");
    let question = find_question(question_id).expect("a real question");
    let order = trivia::decode_order(order).expect("an order");
    let correct = question.correct_slot(order).expect("the answer is shown");
    let incorrect = (correct + 1) % 4;

    let again = sim.command(unique(), "play", json!([sub("trivia", vec![])])).await;
    assert!(
        again.ephemeral && again.content().contains("az önce bir tur oynandı"),
        "{}",
        again.content()
    );
    let click = |slot: usize| format!("tv:{id}:{slot}");
    let locked = sim.component(right, &click(correct), &[], &round).await;
    assert!(
        locked.ephemeral && locked.content().contains("kilitlendi"),
        "{}",
        locked.content()
    );
    let twice = sim.component(right, &click(incorrect), &[], &round).await;
    assert!(
        twice.ephemeral && twice.content().contains("zaten kilitledin"),
        "{}",
        twice.content()
    );
    sim.component(wrong, &click(incorrect), &[], &round).await;

    expire(&sim, id).await;
    let after = sim.component(late, &click(correct), &[], &round).await;
    assert!(
        after.ephemeral && after.content().contains("süre doldu"),
        "{}",
        after.content()
    );
    sweep_games(&sim.app).await.expect("sweep");
    let message = message_of(&sim, id).await;
    let posts = sim.posts();
    let reveal = posts
        .iter()
        .find(|p| p.message == message)
        .expect("the round was revealed");
    assert!(
        reveal.content().contains("**2** kişi cevapladı") && reveal.content().contains(&question.answer),
        "{}",
        reveal.content()
    );
    let embed = reveal.embed();
    assert_eq!(embed["fields"][0]["name"], "Doğru bilenler");
    assert!(
        embed["fields"][0]["value"]
            .as_str()
            .unwrap_or_default()
            .contains(&format!("<@{right}> +{}", question.difficulty.reward()))
    );
    assert_eq!(shells(&sim, right).await, question.difficulty.reward());
    assert_eq!(shells(&sim, wrong).await, 0);

    // A round nobody answers, in the channel's own time.
    sim.channel = unique();
    let quiet = sim
        .command(
            starter,
            "play",
            json!([sub("trivia", vec![text_option("category", "ocean")])]),
        )
        .await;
    let quiet_id = session_of(&quiet);
    // Nobody clicked, so the round does not know its message yet.
    let quiet_message = unique();
    sim.app
        .db
        .set_game_message(quiet_id, CHANNEL_ID, quiet_message)
        .await
        .expect("message");
    expire(&sim, quiet_id).await;
    sweep_games(&sim.app).await.expect("sweep");
    let posts = sim.posts();
    let revealed = posts
        .iter()
        .find(|p| p.message == quiet_message)
        .expect("the quiet round was revealed");
    assert_eq!(revealed.embed()["fields"][0]["value"], "Bu sefer kimse cevap vermedi.");
}

#[tokio::test]
async fn turkish_hangman_is_lost_refuses_repeats_and_is_closed_by_the_janitor() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) = Sim::new("turkish_hangman_is_lost_refuses_repeats_and_is_closed_by_the_janitor").await else {
        return;
    };
    forget_late_games(&sim).await;
    sim.locale = "tr";
    let (loser, other) = (unique(), unique());
    let half = |c: char| {
        if lang::TR_ALPHABET.chars().take(15).any(|a| a == c) {
            "a"
        } else {
            "n"
        }
    };

    let start = sim.command(loser, "play", json!([sub("hangman", vec![])])).await;
    let id = session_of(&start);
    let word = Hangman::decode(&sim.app.db.game(id).await.unwrap().unwrap().state)
        .unwrap()
        .word()
        .to_owned();
    assert_eq!(start.title(), "Kum kalesini kurtar");
    assert!(
        start.content().contains("**6** yanlış hakkı kaldı"),
        "{}",
        start.content()
    );
    let again = sim.command(loser, "play", json!([sub("hangman", vec![])])).await;
    assert!(
        again.ephemeral && again.content().contains("Bir seferde tek kum kalesi"),
        "{}",
        again.content()
    );
    let letter = word.chars().find(|c| *c != ' ').expect("a letter");
    let intruder = sim
        .component(
            other,
            &format!("hm:{id}:{}", half(letter)),
            &[&letter.to_string()],
            &start,
        )
        .await;
    assert!(
        intruder.ephemeral && intruder.content().contains("kum kalesi") && intruder.content().contains("kullanıcısına"),
        "{}",
        intruder.content()
    );

    let mut board = start;
    let misses: Vec<char> = lang::TR_ALPHABET
        .chars()
        .filter(|c| !word.contains(*c))
        .take(6)
        .collect();
    for (n, letter) in misses.iter().enumerate() {
        board = sim
            .pick(
                loser,
                &board,
                &format!("hm:{id}:{}", half(*letter)),
                &letter.to_string(),
            )
            .await;
        if n == 0 {
            assert!(
                board.content().contains("**5** yanlış hakkı kaldı"),
                "{}",
                board.content()
            );
            keep_png(&sim, &board, "hangman-tr-one-miss-letters");
            // A letter that was tried leaves its menu; a stale pick of it is told so.
            let repeat = sim
                .component(
                    loser,
                    &format!("hm:{id}:{}", half(*letter)),
                    &[&letter.to_string()],
                    &board,
                )
                .await;
            assert!(
                repeat.ephemeral && repeat.content().contains("harfini zaten denedin"),
                "{}",
                repeat.content()
            );
        }
    }
    assert_eq!(board.title(), "Deniz kaleyi aldı");
    assert!(
        board.content().contains("Bu sefer gelgit kazandı") && board.content().contains(&Lang::Tr.upper(&word)),
        "{}",
        board.content()
    );
    keep_png(&sim, &board, "hangman-tr-lost");
    assert_eq!(record_of(&sim, loser, "hangman").await, (1, 0));

    // A game nobody plays is closed by the janitor, in the game's language.
    let idle = unique();
    let start = sim.command(idle, "play", json!([sub("hangman", vec![])])).await;
    let idle_id = session_of(&start);
    let message = unique();
    sim.app
        .db
        .set_game_message(idle_id, CHANNEL_ID, message)
        .await
        .expect("message");
    expire(&sim, idle_id).await;
    sweep_games(&sim.app).await.expect("sweep");
    let posts = sim.posts();
    let edit = posts
        .iter()
        .find(|p| p.message == message)
        .expect("the game message was edited");
    assert!(
        edit.content().contains("Kimse bakmazken gelgit geldi"),
        "{}",
        edit.content()
    );
    let late = sim.pick(idle, &start, &format!("hm:{idle_id}:a"), "a").await;
    assert!(
        late.ephemeral && late.content().contains("Bu oyun bitti"),
        "{}",
        late.content()
    );
}

#[tokio::test]
async fn turkish_wordle_shares_once_and_refuses_an_unfinished_share() {
    let Some(mut sim) = Sim::new("turkish_wordle_shares_once_and_refuses_an_unfinished_share").await else {
        return;
    };
    sim.locale = "tr";
    let player = unique();
    let day = today();
    let answer = wordle::daily_word(&PlayContent::for_lang(Lang::Tr).words.answers, day).to_owned();

    let board = sim.command(player, "play", json!([sub("wordle", vec![])])).await;
    let early = sim
        .component(player, &format!("wo:s:daily-tr:{day}"), &[], &board)
        .await;
    assert!(
        early.ephemeral && early.content().contains("Önce bugünkü kelimeyi bitir"),
        "{}",
        early.content()
    );
    let form = sim.click(player, &board, "Tahmin et").await;
    assert_eq!(form.modal.as_ref().map(|m| m.1.as_str()), Some("Wordle tahmini"));
    assert!(form.modal_inputs.iter().any(|(id, _)| id == "guess"));
    let won = sim.submit(player, &form, &answer, &board).await;
    assert!(
        won.description().contains("**1** denemede buldun"),
        "{}",
        won.description()
    );
    let shared = sim.click(player, &won, "Sonucu paylaş").await;
    assert!(
        !shared.ephemeral
            && shared.content().contains("günlük kelime oyununu oynadı")
            && shared.content().contains("Pinchora Wordle TR #"),
        "{}",
        shared.content()
    );
    assert!(shared.pings_nobody() && !shared.content().contains(&answer));
    let twice = sim.click(player, &won, "Sonucu paylaş").await;
    assert!(
        twice.ephemeral && twice.content().contains("zaten paylaştın"),
        "{}",
        twice.content()
    );
    // A finished board takes no more guesses.
    let late = sim.submit(player, &form, &answer, &won).await;
    assert!(
        late.ephemeral && late.content().contains("Bu tahta bitti"),
        "{}",
        late.content()
    );
}
