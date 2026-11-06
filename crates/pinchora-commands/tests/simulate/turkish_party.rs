//! The party games in Turkish, chrome and all: the lobby and its buttons, Hot Pearl, Acronym Tide, Caption the
//! Postcard, Wordle Race, Would You Rather, Truth or Dare, `/party stop`, the filter settings and the refusals.
//! What the words and prompts themselves are is covered next door in `words.rs`.

use pinchora_core::i18n::Lang;
use pinchora_game::{lang, party::writing_reward};

use super::{
    words::{
        PARTY, WORDS_TR, advance, balance, description, edits, fresh, game, game_id, keep, keep_reply, lobby_of, pin,
        st_prompt, state, word_tr,
    },
    *,
};

/// Keeps the picture of a reply under a name that says what it shows.
fn keep_png(sim: &Sim, reply: &Reply, name: &str) {
    keep_reply(sim, name, reply);
}

/// The Turkish phrase for `letters`: one word each.
fn phrase_tr(letters: &str) -> String {
    letters.chars().map(word_tr).collect::<Vec<_>>().join(" ")
}

/// The number on the vote button of `author`'s entry in round 1 of game `id`.
async fn number(sim: &Sim, id: i64, author: u64) -> String {
    let entries = sim.app.db.party_entries(id, 1).await.expect("entries");
    (entries.iter().position(|e| e.author() == author).expect("an entry") + 1).to_string()
}

#[tokio::test]
async fn a_turkish_hot_pearl_goes_from_lobby_to_a_winner() {
    let _alone = PARTY.lock().await;
    let Some(mut sim) = Sim::new("a_turkish_hot_pearl_goes_from_lobby_to_a_winner").await else {
        return;
    };
    fresh(&sim).await;
    sim.channel = unique();
    sim.locale = "tr";
    let (host, ann, bo) = (unique(), unique(), unique());

    let lobby = sim
        .command(host, "party", json!([sub("hotpearl", vec![int_option("lives", 1)])]))
        .await;
    assert!(!lobby.ephemeral && lobby.pings_nobody());
    assert!(lobby.title().contains("Sıcak İnci"), "{}", lobby.title());
    assert!(lobby.content().contains("lobisi açtı"), "{}", lobby.content());
    assert!(lobby.content().contains("(1/8)"), "{}", lobby.content());
    assert!(
        lobby.description().contains("Kurucu:") && lobby.description().contains("herkese 1 can"),
        "{}",
        lobby.description()
    );
    let labels: Vec<_> = ["Katıl", "Ayrıl", "Başlat", "İptal"]
        .into_iter()
        .filter(|l| lobby.has_button(l))
        .collect();
    assert_eq!(labels.len(), 4, "the four lobby buttons are Turkish: {labels:?}");
    keep_png(&sim, &lobby, "lobby-tr");
    let id = game_id(&lobby);
    pin(&sim, id).await;

    // One game at a time in a channel; too few players; only the host starts; the host can not just leave.
    let busy = sim.command(ann, "party", json!([sub("acronym", vec![])])).await;
    assert!(
        busy.ephemeral && busy.content().contains("Sıcak İnci bu kanalda zaten sürüyor"),
        "{}",
        busy.content()
    );
    let early = sim.click(host, &lobby, "Başlat").await;
    assert!(
        early.ephemeral && early.content().contains("en az 2 oyuncu"),
        "{}",
        early.content()
    );
    let lobby = sim.click(ann, &lobby, "Katıl").await;
    assert!(
        !lobby.ephemeral && lobby.content().contains("(2/8)"),
        "{}",
        lobby.content()
    );
    keep_png(&sim, &lobby, "lobby-two-tr");
    let twice = sim.click(ann, &lobby, "Katıl").await;
    assert!(
        twice.ephemeral && twice.content().contains("Zaten içeridesin"),
        "{}",
        twice.content()
    );
    let nope = sim.click(ann, &lobby, "Başlat").await;
    assert!(
        nope.ephemeral && nope.content().contains("yalnızca"),
        "{}",
        nope.content()
    );
    let lobby = sim.click(bo, &lobby, "Katıl").await;
    let left = sim.click(bo, &lobby, "Ayrıl").await;
    assert!(left.content().contains("(2/8)"), "{}", left.content());
    let notin = sim.click(bo, &left, "Ayrıl").await;
    assert!(
        notin.ephemeral && notin.content().contains("değilsin"),
        "{}",
        notin.content()
    );
    let lobby = sim.click(bo, &left, "Katıl").await;
    assert!(lobby.content().contains("(3/8)"), "{}", lobby.content());
    let stuck = sim.click(host, &lobby, "Ayrıl").await;
    assert!(
        stuck.ephemeral && stuck.content().contains("İptal"),
        "{}",
        stuck.content()
    );

    let table = sim.click(host, &lobby, "Başlat").await;
    assert_eq!(table.files[0].0, "pearl.png");
    assert!(table.title().contains("Sıcak İnci"), "{}", table.title());
    assert!(
        table.description().contains("inci patlamadan") && table.description().contains("başlıyor"),
        "{}",
        table.description()
    );
    assert!(table.has_button("Kelime yaz"));
    keep_png(&sim, &table, "pearl-table-tr");
    let st = state(&sim, id).await;
    let cur = st["cur"].as_u64().expect("a holder");
    let syllable = st["syl"].as_str().expect("a syllable").to_owned();
    let others: Vec<u64> = [host, ann, bo].into_iter().filter(|u| *u != cur).collect();

    // Only the holder types, and the form takes real Turkish words with the syllable.
    let wrong = sim.click(others[0], &table, "Kelime yaz").await;
    assert!(
        wrong.ephemeral && wrong.content().contains("Sıra sende değil"),
        "{}",
        wrong.content()
    );
    let form = sim.click(cur, &table, "Kelime yaz").await;
    assert_eq!(
        form.modal.as_ref().map(|m| m.1.as_str()),
        Some("Sıcak İnci"),
        "the form is titled in Turkish"
    );
    let short = sim.submit_fields(cur, &form, &[("word", "ab")]).await;
    assert!(
        short.ephemeral && short.content().contains("en az 3 harfli"),
        "{}",
        short.content()
    );
    let foreign = sim.submit_fields(cur, &form, &[("word", "xyzzy")]).await;
    assert!(
        foreign.ephemeral && foreign.content().contains("Türk alfabesi"),
        "{}",
        foreign.content()
    );
    let without = WORDS_TR
        .lines()
        .find(|w| (4..=8).contains(&w.chars().count()) && !w.contains(syllable.as_str()))
        .expect("a word without the syllable");
    let missing = sim.submit_fields(cur, &form, &[("word", without)]).await;
    assert!(
        missing.ephemeral && missing.content().contains("Bu kelimede"),
        "{}",
        missing.content()
    );
    assert!(
        missing.content().contains(&lang::upper(Lang::Tr, &syllable)),
        "the syllable is shown in capitals: {}",
        missing.content()
    );
    let fake = format!("{syllable}ğüğü");
    let unknown = sim.submit_fields(cur, &form, &[("word", fake.as_str())]).await;
    assert!(
        unknown.ephemeral && unknown.content().contains("listemde yok"),
        "{}",
        unknown.content()
    );
    let good = WORDS_TR
        .lines()
        .find(|w| (4..=8).contains(&w.chars().count()) && w.contains(syllable.as_str()))
        .expect("a word for the syllable")
        .to_owned();
    let played = sim
        .submit_fields(cur, &form, &[("word", lang::upper(Lang::Tr, &good).as_str())])
        .await;
    assert!(!played.ephemeral, "{}", played.content());
    assert!(
        played.description().contains("yazdı") && played.description().contains(&lang::upper(Lang::Tr, &good)),
        "{}",
        played.description()
    );
    keep_png(&sim, &played, "pearl-after-word-tr");
    assert_eq!(state(&sim, id).await["turn"], 1);
    let again = sim.submit_fields(cur, &form, &[("word", good.as_str())]).await;
    assert!(
        again.ephemeral,
        "an old form can not be used again: {}",
        again.content()
    );

    // The clock runs out: the holder (one life) is out, then the last pop leaves one player, who wins. One word
    // was played in all, so nothing is paid, and the result says why.
    sim.posts();
    assert_eq!(advance(&sim, 60).await, 1);
    let posts = sim.posts();
    let popped = edits(&posts);
    assert_eq!(popped.len(), 1);
    assert!(
        description(popped[0]).contains("üzerinde patladı") && description(popped[0]).contains("oyundan çıktı"),
        "{}",
        description(popped[0])
    );
    keep(&sim, "pearl-after-the-pop-tr", popped[0]);
    assert_eq!(advance(&sim, 60).await, 1);
    let posts = sim.posts();
    let ended = edits(&posts);
    assert_eq!(ended.len(), 1);
    assert!(ended[0].title().contains("bitti"), "{}", ended[0].title());
    assert!(
        description(ended[0]).contains("kazandı") && description(ended[0]).contains("Bu sefer kabuk yok"),
        "{}",
        description(ended[0])
    );
    keep(&sim, "pearl-standings-tr", ended[0]);
    let stale = sim.click(cur, &table, "Kelime yaz").await;
    assert!(stale.ephemeral, "{}", stale.content());
}

#[tokio::test]
async fn turkish_party_lobbies_can_be_cancelled_stopped_and_expire() {
    let _alone = PARTY.lock().await;
    let Some(mut sim) = Sim::new("turkish_party_lobbies_can_be_cancelled_stopped_and_expire").await else {
        return;
    };
    fresh(&sim).await;
    sim.channel = unique();
    sim.locale = "tr";
    let (host, ann, bo, stranger, boss) = (unique(), unique(), unique(), unique(), unique());
    let stop = || json!([sub("stop", vec![])]);

    let none = sim.command(host, "party", stop()).await;
    assert!(
        none.ephemeral && none.content().contains("durdurulacak bir parti oyunu yok"),
        "{}",
        none.content()
    );

    // A lobby: a bystander can not cancel it, its host can, and the message closes for good.
    let (lobby, id) = lobby_of(&mut sim, "acronym", vec![int_option("rounds", 2)], host, &[ann]).await;
    assert!(lobby.description().contains("2 tur"), "{}", lobby.description());
    let denied = sim.click(ann, &lobby, "İptal").await;
    assert!(
        denied.ephemeral && denied.content().contains("yalnızca"),
        "{}",
        denied.content()
    );
    let cancelled = sim.click(host, &lobby, "İptal").await;
    assert!(
        cancelled.content().contains("iptal etti") && cancelled.components.is_empty(),
        "{}",
        cancelled.content()
    );
    assert!(game(&sim, id).await.is_done());
    let gone = sim.click(ann, &lobby, "Katıl").await;
    assert!(
        gone.ephemeral && gone.content().contains("kapandı"),
        "{}",
        gone.content()
    );

    // A lobby nobody starts is closed by the clock, with its message.
    let (_, other) = lobby_of(&mut sim, "postcard", vec![], host, &[]).await;
    sim.posts();
    assert_eq!(advance(&sim, 600).await, 1);
    let posts = sim.posts();
    let closed = edits(&posts);
    assert_eq!(closed.len(), 1);
    assert!(
        closed[0].content().contains("süresinde kimse başlatmadı"),
        "{}",
        closed[0].content()
    );
    assert!(game(&sim, other).await.is_done());

    // A game in its second turn, two lives each: the pop leaves a life, a bystander can not stop it, its host can.
    let (lobby, id) = lobby_of(&mut sim, "hotpearl", vec![], host, &[ann, bo]).await;
    let table = sim.click(host, &lobby, "Başlat").await;
    sim.posts();
    assert_eq!(advance(&sim, 60).await, 1);
    let posts = sim.posts();
    let popped = edits(&posts);
    assert_eq!(popped.len(), 1);
    assert!(
        description(popped[0]).contains("1 can kaldı"),
        "{}",
        description(popped[0])
    );
    keep(&sim, "pearl-after-a-pop-tr", popped[0]);
    let late = sim.click(host, &table, "Kelime yaz").await;
    assert!(late.ephemeral, "{}", late.content());
    let denied = sim.command(stranger, "party", stop()).await;
    assert!(
        denied.ephemeral && denied.content().contains("durdurabilir"),
        "{}",
        denied.content()
    );
    assert!(game(&sim, id).await.is_playing(), "a bystander changed nothing");
    sim.posts();
    let stopped = sim.command(host, "party", stop()).await;
    assert!(
        stopped.ephemeral && stopped.content().contains("Durduruldu: Sıcak İnci"),
        "{}",
        stopped.content()
    );
    let posts = sim.posts();
    let closed = edits(&posts);
    assert_eq!(closed.len(), 1);
    assert!(
        closed[0].content().contains("şunu durdurdu: Sıcak İnci") && closed[0].content().contains("kabuk dağıtılmadı"),
        "{}",
        closed[0].content()
    );
    assert!(game(&sim, id).await.is_done());
    assert_eq!(advance(&sim, 600).await, 0);
    for user in [host, ann, bo] {
        assert_eq!(balance(&sim, user).await, 0);
    }

    // A manager stops somebody else's lobby, and the channel is free again.
    let (_, lobby_id) = lobby_of(&mut sim, "postcard", vec![], host, &[]).await;
    sim.permissions = MANAGE_GUILD;
    let stopped = sim.command(boss, "party", stop()).await;
    assert!(
        stopped.content().contains("Durduruldu: Kartpostala Altyazı"),
        "{}",
        stopped.content()
    );
    assert!(game(&sim, lobby_id).await.is_done());
    let next = sim.command(host, "party", json!([sub("wordrace", vec![])])).await;
    assert!(!next.ephemeral, "{}", next.content());
    assert!(next.title().contains("Wordle Yarışı"), "{}", next.title());
}

#[tokio::test]
async fn the_turkish_filter_settings_and_gambling_switch_speak_turkish() {
    let _alone = PARTY.lock().await;
    let Some(mut sim) = Sim::new("the_turkish_filter_settings_and_gambling_switch_speak_turkish").await else {
        return;
    };
    fresh(&sim).await;
    sim.channel = unique();
    sim.locale = "tr";
    let (ada, bo, cy) = (unique(), unique(), unique());
    let config = |on: bool| json!([sub("partygames", vec![bool_option("strict", on)])]);

    // A table that is not strict lets a mild word through to the letter check; a strict one turns it away.
    let (lobby, id) = lobby_of(&mut sim, "acronym", vec![int_option("rounds", 1)], ada, &[bo, cy]).await;
    let round = sim.click(ada, &lobby, "Başlat").await;
    let form = sim.click(ada, &round, "Cümle yaz").await;
    let mild = sim.submit_fields(ada, &form, &[("entry", "salak")]).await;
    assert!(
        mild.ephemeral && mild.content().contains("kelime yazdın"),
        "{}",
        mild.content()
    );
    sim.permissions = MANAGE_GUILD;
    let strict = sim.command(ada, "config", config(true)).await;
    assert!(
        strict.ephemeral
            && strict.content().contains("Parti oyunları filtresi")
            && strict.content().contains("**sıkı**"),
        "{}",
        strict.content()
    );
    let refused = sim.submit_fields(ada, &form, &[("entry", "salak")]).await;
    assert!(
        refused.ephemeral && refused.content().contains("Ortamı güzel tutalım"),
        "{}",
        refused.content()
    );
    let normal = sim.command(ada, "config", config(false)).await;
    assert!(normal.content().contains("**normal**"), "{}", normal.content());
    sim.permissions = 0;
    let rude = sim.submit_fields(ada, &form, &[("entry", "what the fuck")]).await;
    assert!(
        rude.ephemeral && rude.content().contains("Ortamı güzel tutalım"),
        "{}",
        rude.content()
    );
    let link = sim
        .submit_fields(ada, &form, &[("entry", "https://example.com/x")])
        .await;
    assert!(
        link.ephemeral && link.content().contains("Düz kelimelerle"),
        "{}",
        link.content()
    );
    let letters = state(&sim, id).await["letters"].as_str().expect("letters").to_owned();
    let wrong_start = phrase_tr(&letters.chars().rev().collect::<String>());
    if wrong_start != phrase_tr(&letters) {
        let wrong = sim.submit_fields(ada, &form, &[("entry", wrong_start.as_str())]).await;
        assert!(
            wrong.ephemeral && wrong.content().contains("ile başlamalı"),
            "{}",
            wrong.content()
        );
    }

    // The gambling switch is for managers, and says so in Turkish both ways.
    let switch = |on: bool| json!([sub("gambling", vec![bool_option("enabled", on)])]);
    let denied = sim.command(ada, "config", switch(true)).await;
    assert!(
        denied.ephemeral && denied.content().contains("Sunucuyu Yönet"),
        "{}",
        denied.content()
    );
    sim.permissions = MANAGE_GUILD;
    let on = sim.command(ada, "config", switch(true)).await;
    assert!(
        on.content().contains("**açık**") && on.content().contains("`/play shells`"),
        "{}",
        on.content()
    );
    let off = sim.command(ada, "config", switch(false)).await;
    assert!(off.content().contains("**kapalı**"), "{}", off.content());
}

#[tokio::test]
async fn a_turkish_acronym_game_goes_from_letters_to_the_final_scores() {
    let _alone = PARTY.lock().await;
    let Some(mut sim) = Sim::new("a_turkish_acronym_game_goes_from_letters_to_the_final_scores").await else {
        return;
    };
    fresh(&sim).await;
    sim.channel = unique();
    sim.locale = "tr";
    let (ada, bo, cy) = (unique(), unique(), unique());
    let (lobby, id) = lobby_of(&mut sim, "acronym", vec![int_option("rounds", 1)], ada, &[bo]).await;
    assert!(lobby.title().contains("Kısaltma Gelgiti"), "{}", lobby.title());
    assert!(
        lobby.description().contains("Harfler çıkar") && lobby.description().contains("3-10 oyuncu"),
        "{}",
        lobby.description()
    );
    keep_png(&sim, &lobby, "acronym-lobby-tr");
    let early = sim.click(ada, &lobby, "Başlat").await;
    assert!(
        early.ephemeral && early.content().contains("en az 3 oyuncu"),
        "{}",
        early.content()
    );
    let lobby = sim.click(cy, &lobby, "Katıl").await;
    let round = sim.click(ada, &lobby, "Başlat").await;
    assert_eq!(round.files[0].0, "letters.png");
    assert!(
        round.description().contains("harflerinden bir cümle kur"),
        "{}",
        round.description()
    );
    assert!(
        round.title().contains("Tur 1/1") && round.title().contains("Kısaltma Gelgiti"),
        "{}",
        round.title()
    );
    keep_png(&sim, &round, "acronym-letters-tr");
    let letters = state(&sim, id).await["letters"].as_str().expect("letters").to_owned();
    let good = phrase_tr(&letters);

    let stranger = sim.click(unique(), &round, "Cümle yaz").await;
    assert!(
        stranger.ephemeral && stranger.content().contains("Bu oyunda yoksun"),
        "{}",
        stranger.content()
    );
    let form = sim.click(ada, &round, "Cümle yaz").await;
    assert_eq!(form.modal.as_ref().map(|m| m.1.as_str()), Some("Kısaltma Gelgiti"));
    let few = sim.submit_fields(ada, &form, &[("entry", "sadece iki")]).await;
    assert!(
        few.ephemeral && few.content().contains("kelime yazdın") && few.content().contains("gerekiyor"),
        "{}",
        few.content()
    );
    let empty = sim.submit_fields(ada, &form, &[("entry", "<@1>")]).await;
    assert!(
        empty.ephemeral && empty.content().contains("Düz kelimelerle"),
        "{}",
        empty.content()
    );

    // A phrase is locked in privately, and a rewrite replaces it.
    sim.posts();
    let locked = sim.submit_fields(ada, &form, &[("entry", good.as_str())]).await;
    assert!(
        locked.ephemeral && locked.content().contains("Kaydedildi"),
        "{}",
        locked.content()
    );
    let again = sim.submit_fields(ada, &form, &[("entry", good.as_str())]).await;
    assert!(
        again.ephemeral && again.content().contains("Yeniden yazıldı"),
        "{}",
        again.content()
    );
    let posts = sim.posts();
    assert!(
        edits(&posts)
            .iter()
            .any(|p| description(p).contains("3 kişiden 1 cümle geldi")),
        "the message keeps count of the phrases in"
    );
    sim.submit_fields(bo, &form, &[("entry", good.as_str())]).await;
    sim.submit_fields(cy, &form, &[("entry", good.as_str())]).await;
    let posts = sim.posts();
    let vote = edits(&posts).into_iter().last().expect("the vote went up");
    assert!(vote.title().contains("oylama"), "{}", vote.title());
    assert!(
        description(vote).contains("En beğendiğin cümleye oy ver"),
        "{}",
        description(vote)
    );
    keep(&sim, "acronym-vote-tr", vote);
    let ballot = posted_reply(vote);

    // Votes: not for yourself, changeable, counted once each; the last one ends the vote early.
    let own = sim.click(ada, &ballot, &number(&sim, id, ada).await).await;
    assert!(own.ephemeral && own.content().contains("Bu senin"), "{}", own.content());
    let first = sim.click(ada, &ballot, &number(&sim, id, cy).await).await;
    assert!(first.content().contains("oyun kaydedildi"), "{}", first.content());
    let same = sim.click(ada, &ballot, &number(&sim, id, cy).await).await;
    assert!(same.content().contains("Zaten"), "{}", same.content());
    let moved = sim.click(ada, &ballot, &number(&sim, id, bo).await).await;
    assert!(moved.content().contains("değiştirdim"), "{}", moved.content());
    let outsider = sim.click(unique(), &ballot, &number(&sim, id, bo).await).await;
    assert!(
        outsider.content().contains("Bu oyunda yoksun"),
        "{}",
        outsider.content()
    );
    sim.click(bo, &ballot, &number(&sim, id, cy).await).await;
    sim.posts();
    sim.click(cy, &ballot, &number(&sim, id, bo).await).await;
    let posts = sim.posts();
    let reveal = edits(&posts).into_iter().last().expect("the reveal went up");
    assert!(reveal.title().contains("tur sonuçları"), "{}", reveal.title());
    assert!(
        description(reveal).contains("turu") && description(reveal).contains("Final puanları"),
        "{}",
        description(reveal)
    );
    keep(&sim, "acronym-reveal-tr", reveal);
    let late = sim.click(ada, &ballot, &number(&sim, id, bo).await).await;
    assert!(late.ephemeral, "{}", late.content());

    assert_eq!(advance(&sim, 30).await, 1);
    let posts = sim.posts();
    let end = edits(&posts).into_iter().last().expect("the final scores went up");
    assert!(end.title().contains("bitti"), "{}", end.title());
    assert_eq!(end.files[0].0, "standings.png");
    assert!(description(end).contains("kazandı"), "{}", description(end));
    keep(&sim, "acronym-standings-tr", end);
    assert_eq!(balance(&sim, bo).await, writing_reward(4));
}

#[tokio::test]
async fn a_turkish_postcard_game_shows_the_postcard_votes_and_scores() {
    let _alone = PARTY.lock().await;
    let Some(mut sim) = Sim::new("a_turkish_postcard_game_shows_the_postcard_votes_and_scores").await else {
        return;
    };
    fresh(&sim).await;
    sim.channel = unique();
    sim.locale = "tr";
    let (ada, bo, cy) = (unique(), unique(), unique());
    let (lobby, id) = lobby_of(&mut sim, "postcard", vec![int_option("rounds", 1)], ada, &[bo, cy]).await;
    assert!(lobby.title().contains("Kartpostala Altyazı"), "{}", lobby.title());
    assert!(
        lobby.description().contains("Piksel bir kartpostal çıkar"),
        "{}",
        lobby.description()
    );
    let round = sim.click(ada, &lobby, "Başlat").await;
    assert_eq!(round.files[0].0, "postcard.png");
    assert!(
        round.description().contains("Bu kartpostal için bir altyazı yaz"),
        "{}",
        round.description()
    );
    keep_png(&sim, &round, "postcard-tr");
    let form = sim.click(ada, &round, "Altyazı yaz").await;
    assert_eq!(
        form.modal.as_ref().map(|m| m.1.as_str()),
        Some("Kartpostala altyazı yaz")
    );
    let short = sim.submit_fields(ada, &form, &[("entry", "ah")]).await;
    assert!(
        short.ephemeral && short.content().contains("en az 3 karakter"),
        "{}",
        short.content()
    );
    let long = sim
        .submit_fields(ada, &form, &[("entry", "a".repeat(120).as_str())])
        .await;
    assert!(
        long.ephemeral && long.content().contains("karakteri geçmesin"),
        "{}",
        long.content()
    );
    for (user, caption) in [
        (ada, "Yengeç beni sınıfta bıraktı"),
        (bo, "Kimse bana kumdan kale yapmayı öğretmedi"),
        (cy, "Bugün tatil değil mi?"),
    ] {
        let locked = sim.submit_fields(user, &form, &[("entry", caption)]).await;
        assert!(
            locked.ephemeral && locked.content().contains("Kaydedildi"),
            "{}",
            locked.content()
        );
    }
    let posts = sim.posts();
    let vote = edits(&posts).into_iter().last().expect("the vote went up");
    assert!(vote.title().contains("oylama"), "{}", vote.title());
    keep(&sim, "postcard-vote-tr", vote);
    let ballot = posted_reply(vote);
    sim.click(ada, &ballot, &number(&sim, id, cy).await).await;
    sim.click(bo, &ballot, &number(&sim, id, cy).await).await;
    sim.posts();
    sim.click(cy, &ballot, &number(&sim, id, ada).await).await;
    let posts = sim.posts();
    let reveal = edits(&posts).into_iter().last().expect("the reveal went up");
    assert!(reveal.title().contains("tur sonuçları"), "{}", reveal.title());
    assert!(description(reveal).contains("kazandı"), "{}", description(reveal));
    keep(&sim, "postcard-reveal-tr", reveal);
    assert_eq!(advance(&sim, 30).await, 1);
    let posts = sim.posts();
    let end = edits(&posts).into_iter().last().expect("the final scores");
    assert!(end.title().contains("bitti"), "{}", end.title());
    keep(&sim, "postcard-standings-tr", end);
    assert_eq!(balance(&sim, cy).await, writing_reward(4));
}

#[tokio::test]
async fn a_turkish_word_race_has_a_lobby_private_boards_and_a_result() {
    let _alone = PARTY.lock().await;
    let Some(mut sim) = Sim::new("a_turkish_word_race_has_a_lobby_private_boards_and_a_result").await else {
        return;
    };
    fresh(&sim).await;
    sim.channel = unique();
    sim.locale = "tr";
    let (ada, bo, cy, outsider) = (unique(), unique(), unique(), unique());
    let (lobby, id) = lobby_of(&mut sim, "wordrace", vec![], ada, &[bo, cy]).await;
    assert!(lobby.title().contains("Wordle Yarışı"), "{}", lobby.title());
    assert!(
        lobby.description().contains("kişi başı altı tahmin"),
        "{}",
        lobby.description()
    );
    let race = sim.click(ada, &lobby, "Başlat").await;
    assert_eq!(race.files[0].0, "race.png");
    assert!(
        race.description().contains("Aynı beş harfli kelimeyi") && race.has_button("Tahmin et"),
        "{}",
        race.description()
    );
    keep_png(&sim, &race, "race-start-tr");
    let word = state(&sim, id).await["word"].as_str().expect("the word").to_owned();

    let outside = sim.click(outsider, &race, "Tahmin et").await;
    assert!(
        outside.ephemeral && outside.content().contains("Bu yarışta değilsin"),
        "{}",
        outside.content()
    );
    let form = sim.click(bo, &race, "Tahmin et").await;
    assert_eq!(form.modal.as_ref().map(|m| m.1.as_str()), Some("Wordle Yarışı tahmini"));
    let short = sim.submit_fields(bo, &form, &[("guess", "abc")]).await;
    assert!(
        short.ephemeral && short.content().contains("beş harf"),
        "{}",
        short.content()
    );
    let unknown = sim.submit_fields(bo, &form, &[("guess", "zzzzz")]).await;
    assert!(
        unknown.ephemeral && unknown.content().contains("listemde yok"),
        "{}",
        unknown.content()
    );

    let other = PlayContent::for_lang(Lang::Tr)
        .words
        .allowed
        .iter()
        .find(|w| **w != word && w.contains('ş') && w.contains('ı'))
        .expect("another word")
        .clone();
    sim.posts();
    let board = sim
        .submit_fields(bo, &form, &[("guess", lang::upper(Lang::Tr, &other).as_str())])
        .await;
    assert!(
        board.ephemeral && board.files[0].0 == "board.png",
        "{}",
        board.content()
    );
    assert!(board.title().contains("Tahtan"), "{}", board.title());
    assert!(board.description().contains("doğru yer"), "{}", board.description());
    keep_png(&sim, &board, "race-private-board-tr");
    let posts = sim.posts();
    let progress = edits(&posts).into_iter().last().expect("the card moved on");
    assert_eq!(progress.files[0].0, "race.png");
    keep(&sim, "race-progress-tr", progress);

    // The first to find the word wins; the race ends for everyone and shows the word.
    sim.submit_fields(ada, &form, &[("guess", other.as_str())]).await;
    sim.posts();
    let win = sim.submit_fields(ada, &form, &[("guess", word.as_str())]).await;
    assert!(
        win.ephemeral && win.description().contains("denemede buldun"),
        "{}",
        win.description()
    );
    let posts = sim.posts();
    let end = edits(&posts).into_iter().last().expect("the result");
    assert!(end.title().contains("bitti"), "{}", end.title());
    assert!(
        description(end).contains("kelimeyi ilk buldu") && description(end).contains(&lang::upper(Lang::Tr, &word)),
        "{}",
        description(end)
    );
    keep(&sim, "race-standings-tr", end);
    let late = sim.submit_fields(cy, &form, &[("guess", other.as_str())]).await;
    assert!(
        late.ephemeral && late.content().contains("Bu yarış bitti"),
        "{}",
        late.content()
    );
}

#[tokio::test]
async fn a_turkish_race_nobody_wins_ends_with_the_clock() {
    let _alone = PARTY.lock().await;
    let Some(mut sim) = Sim::new("a_turkish_race_nobody_wins_ends_with_the_clock").await else {
        return;
    };
    fresh(&sim).await;
    sim.channel = unique();
    sim.locale = "tr";
    let (ada, bo) = (unique(), unique());
    let (lobby, id) = lobby_of(&mut sim, "wordrace", vec![], ada, &[bo]).await;
    let race = sim.click(ada, &lobby, "Başlat").await;
    sim.posts();
    assert_eq!(advance(&sim, 600).await, 1);
    let posts = sim.posts();
    let end = edits(&posts).into_iter().last().expect("the result");
    assert!(
        description(end).contains("kimse kelimeyi bulamadı"),
        "{}",
        description(end)
    );
    keep(&sim, "race-nobody-tr", end);
    let form = sim.click(ada, &race, "Tahmin et").await;
    assert!(
        form.ephemeral && form.content().contains("Bu yarış bitti"),
        "{}",
        form.content()
    );
    assert!(game(&sim, id).await.is_done());
}

#[tokio::test]
async fn a_turkish_would_you_rather_poll_takes_votes_and_shows_the_split() {
    let _alone = PARTY.lock().await;
    let Some(mut sim) = Sim::new("a_turkish_would_you_rather_poll_takes_votes_and_shows_the_split").await else {
        return;
    };
    fresh(&sim).await;
    sim.channel = unique();
    sim.locale = "tr";
    let (ada, bo, cy) = (unique(), unique(), unique());
    let poll = sim
        .command(
            ada,
            "party",
            json!([sub("wouldyourrather", vec![text_option("topic", "ocean")])]),
        )
        .await;
    assert!(!poll.ephemeral && poll.pings_nobody());
    assert!(poll.title().contains("Hangisini seçerdin?"), "{}", poll.title());
    assert!(poll.description().contains("Bir taraf seç"), "{}", poll.description());
    assert!(poll.has_button_starting("A · ") && poll.has_button_starting("B · "));
    keep_png(&sim, &poll, "rather-open-tr");
    let id = game_id(&poll);
    pin(&sim, id).await;
    let waits = sim.command(bo, "party", json!([sub("wouldyourrather", vec![])])).await;
    assert!(
        waits.ephemeral && waits.content().contains("az önce bir soru gönderildi"),
        "{}",
        waits.content()
    );

    let first = sim.click_starting(ada, &poll, "A · ").await;
    assert!(
        first.ephemeral && first.content().contains("**A** seçtin"),
        "{}",
        first.content()
    );
    assert!(first.content().contains("+1 kabuk"), "{}", first.content());
    let same = sim.click_starting(ada, &poll, "A · ").await;
    assert!(same.content().contains("Zaten **A**"), "{}", same.content());
    sim.click_starting(bo, &poll, "B · ").await;
    sim.click_starting(cy, &poll, "B · ").await;
    let moved = sim.click_starting(ada, &poll, "B · ").await;
    assert!(moved.content().contains("taşıdın"), "{}", moved.content());
    assert!(
        moved.content().contains("B %100") && moved.content().contains("3 oy"),
        "{}",
        moved.content()
    );

    sim.posts();
    assert_eq!(advance(&sim, 700).await, 1);
    let posts = sim.posts();
    let closed = edits(&posts).into_iter().last().expect("the result");
    assert!(
        description(closed).contains("Anket kapandı: toplam **3** oy"),
        "{}",
        description(closed)
    );
    keep(&sim, "rather-result-tr", closed);
    let late = sim.click_starting(bo, &poll, "A · ").await;
    assert!(
        late.ephemeral && late.content().contains("Bu anket kapandı"),
        "{}",
        late.content()
    );
}

#[tokio::test]
async fn a_turkish_truth_or_dare_table_takes_turns_and_ends() {
    let _alone = PARTY.lock().await;
    let Some(mut sim) = Sim::new("a_turkish_truth_or_dare_table_takes_turns_and_ends").await else {
        return;
    };
    fresh(&sim).await;
    sim.channel = unique();
    sim.locale = "tr";
    let (ada, bo, cy) = (unique(), unique(), unique());
    let (lobby, id) = lobby_of(
        &mut sim,
        "truthordare",
        vec![text_option("topic", "cozy")],
        ada,
        &[bo, cy],
    )
    .await;
    assert!(lobby.title().contains("Doğruluk mu Cesaret mi"), "{}", lobby.title());
    assert!(lobby.description().contains("aileye uygun"), "{}", lobby.description());
    let turn = sim.click(ada, &lobby, "Başlat").await;
    assert_eq!(turn.files[0].0, "turn.png");
    assert!(
        turn.description().contains("doğruluk mu cesaret mi?"),
        "{}",
        turn.description()
    );
    assert!(turn.has_button("Doğruluk") && turn.has_button("Cesaret") && turn.has_button("Oyunu bitir"));
    keep_png(&sim, &turn, "dare-turn-tr");
    let st = state(&sim, id).await;
    let first = st["cur"].as_u64().expect("somebody is up");
    let order: Vec<u64> = st["order"]
        .as_array()
        .expect("an order")
        .iter()
        .map(|v| v.as_u64().unwrap())
        .collect();

    let not_you = sim.click(order[1], &turn, "Doğruluk").await;
    assert!(
        not_you.ephemeral && not_you.content().contains("Sıra sende değil"),
        "{}",
        not_you.content()
    );
    let truth = sim.click(first, &turn, "Doğruluk").await;
    assert!(!truth.ephemeral);
    assert!(
        truth.description().contains("bir doğruluk geldi") && truth.has_button("Tamam") && truth.has_button("Pas"),
        "{}",
        truth.description()
    );
    assert!(st_prompt(&sim, id).await.starts_with("truth_cozy_tr_"));
    keep_png(&sim, &truth, "truth-tr");
    let skipped = sim.click(order[1], &truth, "Tamam").await;
    assert!(skipped.ephemeral, "{}", skipped.content());
    let done = sim.click(first, &truth, "Tamam").await;
    assert!(
        done.description().contains("yaptı!") && done.description().contains("+4 kabuk"),
        "{}",
        done.description()
    );
    assert_eq!(balance(&sim, first).await, 4);

    let dare = sim.click(order[1], &done, "Cesaret").await;
    assert!(
        dare.description().contains("bir cesaret geldi"),
        "{}",
        dare.description()
    );
    keep_png(&sim, &dare, "dare-tr");
    let passed = sim.click(order[1], &dare, "Pas").await;
    assert!(passed.description().contains("pas geçti"), "{}", passed.description());
    let denied = sim.click(bo, &passed, "Oyunu bitir").await;
    assert!(
        denied.ephemeral && denied.content().contains("yalnızca"),
        "{}",
        denied.content()
    );

    // Nobody answers for two turns in a row: the game ends by itself.
    sim.posts();
    assert_eq!(advance(&sim, 200).await, 1);
    let posts = sim.posts();
    let quiet = edits(&posts).into_iter().last().expect("a quiet turn");
    assert!(
        description(quiet).contains("zamanında cevap vermedi"),
        "{}",
        description(quiet)
    );
    keep(&sim, "dare-quiet-turn-tr", quiet);
    assert_eq!(advance(&sim, 200).await, 1);
    let posts = sim.posts();
    let over = edits(&posts).into_iter().last().expect("the end");
    assert!(over.title().contains("bitti"), "{}", over.title());
    assert!(description(over).contains("Masa sessizleşti"), "{}", description(over));
    keep(&sim, "dare-standings-tr", over);
    assert!(game(&sim, id).await.is_done());

    // A second game: the host ends it with the button.
    let (lobby, other) = lobby_of(&mut sim, "truthordare", vec![], ada, &[bo]).await;
    let turn = sim.click(ada, &lobby, "Başlat").await;
    let ended = sim.click(ada, &turn, "Oyunu bitir").await;
    assert!(
        ended.title().contains("bitti") && ended.components.is_empty(),
        "{}",
        ended.title()
    );
    assert!(
        ended.description().contains("Kurucu oyunu bitirdi"),
        "{}",
        ended.description()
    );
    assert!(game(&sim, other).await.is_done());
}

#[test]
fn the_party_catalog_has_a_place_for_every_postcard_and_a_name_for_every_topic() {
    for scene in pinchora_game::party::postcard::SCENES {
        for lang in Lang::ALL {
            let key = format!("postcard.place.{}", scene.id);
            let place = pinchora_core::t!(lang, &key);
            assert_ne!(place, key, "{lang:?} lacks {key}");
            assert!(place.chars().count() <= 24, "{place}");
        }
        assert_eq!(
            pinchora_core::t!(Lang::En, &format!("postcard.place.{}", scene.id)),
            scene.place
        );
    }
    for category in pinchora_game::party::prompts::Category::ALL {
        assert_eq!(category.label_in(Lang::En), category.label());
        assert_ne!(category.label_in(Lang::Tr), category.label_in(Lang::En));
    }
}
