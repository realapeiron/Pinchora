//! The crab derby, the treasure heist, the shell game, the storm surge and `/play stats` in Turkish: lobbies,
//! buttons, results, refusals, and the janitor speaking the language a table was opened in. Pictures land in
//! `target/sim/<test>/`.

use pinchora_game::party::{
    derby::ROSTER,
    heist::{self, Role, Tier},
};

use super::*;

/// How Discord writes a mention.
fn at(user: u64) -> String {
    format!("<@{user}>")
}

/// Keeps the picture of a reply next to the sim's own, under a name that says what it shows.
fn keep_png(sim: &Sim, reply: &Reply, name: &str) {
    let (_, png) = reply.files.first().expect("the reply carried a picture");
    std::fs::write(sim.dir.join(format!("{name}.png")), png).expect("save the picture");
}

#[tokio::test]
async fn a_turkish_free_derby_names_the_crabs_runs_the_race_and_answers_in_turkish() {
    let Some(mut sim) = Sim::new("a_turkish_free_derby_names_the_crabs_runs_the_race_and_answers_in_turkish").await
    else {
        return;
    };
    sim.channel = unique();
    sim.locale = "tr";
    turkish_server(&sim).await;
    let (host, a, b, stranger) = (unique(), unique(), unique(), unique());
    let lobby = open_derby(&mut sim, host, None).await;
    assert!(
        !lobby.ephemeral
            && lobby.content().contains("Keşiş Yengeç Yarışı")
            && lobby.content().contains("Yatırım yok, sadece övünme hakkı"),
        "{}",
        lobby.content()
    );
    assert_eq!(lobby.title(), "Keşiş Yengeç Yarışı");
    assert!(lobby.pings_nobody());
    assert_png(&lobby, 600, 470);
    keep_png(&sim, &lobby, "derby-tr-lobby");
    for label in ["Yarışı başlat", "Ayrıl"] {
        assert!(lobby.has_button(label), "{label}");
    }
    let session = session_of(&lobby);
    let state = sim.app.db.game(session).await.unwrap().unwrap().state;
    assert!(state.ends_with("|tr"), "the lobby remembers its language: {state}");
    let race = race_of(&sim, session).await;
    let (winner, other) = (race.winner(), (race.winner() + 1) % 6);
    let names = ROSTER.map(|c| c.name);

    let second = open_derby(&mut sim, stranger, None).await;
    assert!(
        second.ephemeral && second.content().contains("zaten açık bir yarış"),
        "{}",
        second.content()
    );
    let after_a = sim.click(a, &lobby, names[winner]).await;
    let after_b = sim.click(b, &after_a, names[other]).await;
    keep_png(&sim, &after_b, "derby-tr-picked");
    let early = sim.click(stranger, &after_b, "Yarışı başlat").await;
    assert!(
        early.ephemeral && early.content().contains("yalnızca"),
        "{}",
        early.content()
    );
    let nobody = sim.click(stranger, &after_b, "Ayrıl").await;
    assert!(
        nobody.ephemeral && nobody.content().contains("henüz bir yengeç seçmedin"),
        "{}",
        nobody.content()
    );
    let finished = sim.click(host, &after_b, "Yarışı başlat").await;
    assert!(finished.components.is_empty());
    assert!(
        finished.content().contains("Keşiş Yengeç Yarışı")
            && finished.content().contains("||")
            && finished.content().contains(&format!("**{}**", names[winner]))
            && finished.content().contains(&format!("{} +20", at(a)))
            && finished.content().contains("tutanlar"),
        "{}",
        finished.content()
    );
    assert_eq!(finished.title(), "Keşiş Yengeç Yarışı");
    let frames = gif_frames(&finished, "derby.gif");
    save_frames(&sim, "derby-tr", &frames);
    frames[frames.len() - 1]
        .save(sim.dir.join("derby-tr-last.png"))
        .expect("save the last frame");
    assert_eq!((shells(&sim, a).await, shells(&sim, b).await), (20, 0));
    let late = sim.click(a, &after_b, names[other]).await;
    assert!(late.ephemeral && late.content().contains("bitti"), "{}", late.content());
}

#[tokio::test]
async fn a_staked_turkish_derby_is_refused_until_gambling_is_on_and_the_janitor_keeps_the_lobbys_language() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) =
        Sim::new("a_staked_turkish_derby_is_refused_until_gambling_is_on_and_the_janitor_keeps_the_lobbys_language")
            .await
    else {
        return;
    };
    forget_late_games(&sim).await;
    sim.channel = unique();
    sim.locale = "tr";
    turkish_server(&sim).await;
    let (host, a, b, c, broke) = (unique(), unique(), unique(), unique(), unique());
    let off = open_derby(&mut sim, host, Some(40)).await;
    assert!(
        off.ephemeral && off.content().contains("**kapalı**") && off.content().contains("/setup economy gambling"),
        "{}",
        off.content()
    );
    gambling(&mut sim, unique(), true).await;
    for user in [a, b, c] {
        fund(&sim, user, 100).await;
    }
    fund(&sim, broke, 10).await;
    let lobby = open_derby(&mut sim, host, Some(40)).await;
    keep_png(&sim, &lobby, "derby-tr-staked-lobby");
    assert!(
        lobby.content().contains("**40** kabuk yatırıyor"),
        "{}",
        lobby.content()
    );
    let session = session_of(&lobby);
    let race = race_of(&sim, session).await;
    let names = ROSTER.map(|c| c.name);
    let (win, lose) = (race.winner(), (race.winner() + 1) % 6);
    let short = sim.click(broke, &lobby, names[win]).await;
    assert!(
        short.ephemeral && short.content().contains("Yalnızca **10** kabuğun var"),
        "{}",
        short.content()
    );
    sim.click(a, &lobby, names[win]).await;
    sim.click(b, &lobby, names[win]).await;
    sim.click(c, &lobby, names[lose]).await;

    // Nobody starts it; the janitor runs the race and edits the message in the language of the lobby, which
    // an English-locale click on it does not change.
    sim.locale = "en-US";
    let message = give_message(&sim, session).await;
    expire(&sim, session).await;
    sweep_games(&sim.app).await.expect("sweep");
    let posts = sim.posts();
    let edit = edit_with(&posts, message, "derby.gif");
    assert!(
        edit.content().contains("**120** kabukluk havuzu paylaşıyor")
            && edit.content().contains(&format!("{} +60", at(a)))
            && !edit.content().contains("share the pool"),
        "{}",
        edit.content()
    );
    assert_eq!(
        (shells(&sim, a).await, shells(&sim, b).await, shells(&sim, c).await),
        (120, 120, 60)
    );

    // Too few racers for a staked race: called off, the stake goes back.
    sim.locale = "tr";
    let lobby = open_derby(&mut sim, unique(), Some(25)).await;
    sim.click(a, &lobby, names[win]).await;
    let message = give_message(&sim, session_of(&lobby)).await;
    expire(&sim, session_of(&lobby)).await;
    sweep_games(&sim.app).await.expect("sweep");
    let posts = sim.posts();
    let post = posts
        .iter()
        .find(|p| p.message == message)
        .expect("the lobby was edited");
    assert!(
        post.content().contains("iptal edildi")
            && post.content().contains("en az 2 yarışçı gerekir")
            && post.content().contains("Yatırılan kabuklar geri döndü"),
        "{}",
        post.content()
    );
    assert_eq!(shells(&sim, a).await, 120, "the stake came back");
}

#[tokio::test]
async fn a_turkish_heist_crew_picks_roles_and_the_vault_pays() {
    let Some(mut sim) = Sim::new("a_turkish_heist_crew_picks_roles_and_the_vault_pays").await else {
        return;
    };
    sim.channel = unique();
    sim.locale = "tr";
    turkish_server(&sim).await;
    let (host, a, b, stranger) = (unique(), unique(), unique(), unique());
    let lobby = open_heist(&mut sim, host).await;
    assert!(
        !lobby.ephemeral
            && lobby.content().contains("**Hazine Soygunu**")
            && lobby.content().contains("Kimse kabuk kaybetmez"),
        "{}",
        lobby.content()
    );
    assert_eq!(lobby.title(), "Hazine Soygunu");
    assert!(lobby.description().contains("**Gözcü**") && lobby.description().contains("istiridye kilidini açar"));
    assert!(lobby.pings_nobody());
    assert_png(&lobby, 600, 476);
    keep_png(&sim, &lobby, "heist-tr-lobby");
    for label in ["Gözcü", "Çilingir", "Dalgıç", "Hemen yola çık", "Ayrıl"] {
        assert!(lobby.has_button(label), "{label}");
    }
    let session = session_of(&lobby);
    let second = open_heist(&mut sim, stranger).await;
    assert!(
        second.ephemeral && second.content().contains("zaten bir soygun planlanıyor"),
        "{}",
        second.content()
    );

    let crew_roles = [Role::Lookout, Role::Locksmith, Role::Diver];
    let seed = (0..10_000u64)
        .find(|s| heist::run(*s, &crew_roles).tier == Tier::Vault)
        .expect("some seed opens the vault");
    sqlx::query("UPDATE game_sessions SET state = $2 WHERE id = $1")
        .bind(session)
        .bind(format!("{seed}|tr"))
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    let view = sim.click(host, &lobby, "Gözcü").await;
    assert_png(&view, 600, 476);
    keep_png(&sim, &view, "heist-tr-one-in");
    let view = sim.click(a, &view, "Çilingir").await;
    let view = sim.click(b, &view, "Dalgıç").await;
    keep_png(&sim, &view, "heist-tr-full-crew");
    let early = sim.click(stranger, &view, "Hemen yola çık").await;
    assert!(
        early.ephemeral && early.content().contains("yalnızca"),
        "{}",
        early.content()
    );
    let nobody = sim.click(stranger, &view, "Ayrıl").await;
    assert!(
        nobody.ephemeral && nobody.content().contains("Bu ekipte değilsin"),
        "{}",
        nobody.content()
    );
    let done = sim.click(host, &view, "Hemen yola çık").await;
    assert_png(&done, 600, 504);
    keep_png(&sim, &done, "heist-tr-vault");
    assert!(
        done.content().contains("soygunu bitti: **kasa açıldı**") && done.components.is_empty(),
        "{}",
        done.content()
    );
    assert_eq!(done.title(), "Hazine Soygunu");
    for (user, icon) in [(host, "🔭"), (a, "🔑"), (b, "🤿")] {
        assert!(
            done.content().contains(&format!("{icon} {} **+60**", at(user))),
            "{}",
            done.content()
        );
    }
    let late = sim.click(a, &view, "Gözcü").await;
    assert!(late.ephemeral && late.content().contains("bitti"), "{}", late.content());
    let resting = open_heist(&mut sim, host).await;
    assert!(
        resting.ephemeral && resting.content().contains("ortalıkta görünmüyorsun"),
        "{}",
        resting.content()
    );
}

#[tokio::test]
async fn a_busted_turkish_heist_and_a_lone_crew_member_read_in_turkish_even_when_the_janitor_ends_them() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) =
        Sim::new("a_busted_turkish_heist_and_a_lone_crew_member_read_in_turkish_even_when_the_janitor_ends_them").await
    else {
        return;
    };
    forget_late_games(&sim).await;
    sim.channel = unique();
    sim.locale = "tr";
    turkish_server(&sim).await;
    let (host, a, loner) = (unique(), unique(), unique());
    let lobby = open_heist(&mut sim, host).await;
    let session = session_of(&lobby);
    let crew_roles = [Role::Diver, Role::Diver];
    let seed = (0..10_000u64)
        .find(|s| heist::run(*s, &crew_roles).tier == Tier::Busted)
        .expect("some seed fails this crew");
    sqlx::query("UPDATE game_sessions SET state = $2 WHERE id = $1")
        .bind(session)
        .bind(format!("{seed}|tr"))
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    let view = sim.click(host, &lobby, "Dalgıç").await;
    sim.click(a, &view, "Dalgıç").await;
    let message = give_message(&sim, session).await;
    expire(&sim, session).await;
    sim.locale = "en-US";
    sweep_games(&sim.app).await.expect("sweep");
    let posts = sim.posts();
    let edit = edit_with(&posts, message, "heist.png");
    assert!(
        edit.content().contains("soygunu bitti: **yakalandılar**") && !edit.content().contains("heist is over"),
        "{}",
        edit.content()
    );
    let (_, png) = edit.files.first().expect("the card");
    std::fs::write(sim.dir.join("heist-tr-busted.png"), png).expect("save the picture");

    // One crew member is not a crew.
    sim.locale = "tr";
    let lobby = open_heist(&mut sim, unique()).await;
    let session = session_of(&lobby);
    sim.click(loner, &lobby, "Gözcü").await;
    let message = give_message(&sim, session).await;
    expire(&sim, session).await;
    sweep_games(&sim.app).await.expect("sweep");
    let posts = sim.posts();
    let post = posts
        .iter()
        .find(|p| p.message == message)
        .expect("the lobby was edited");
    assert!(
        post.content().contains("soygunu iptal edildi") && post.content().contains("en az 2 kişi gerekir"),
        "{}",
        post.content()
    );
}

#[tokio::test]
async fn the_turkish_shell_game_is_off_until_a_manager_turns_it_on_and_tells_where_the_pearl_was() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) =
        Sim::new("the_turkish_shell_game_is_off_until_a_manager_turns_it_on_and_tells_where_the_pearl_was").await
    else {
        return;
    };
    forget_late_games(&sim).await;
    sim.channel = unique();
    sim.locale = "tr";
    turkish_server(&sim).await;
    let (manager, player, loser) = (unique(), unique(), unique());
    fund(&sim, player, 200).await;
    fund(&sim, loser, 200).await;
    let shells_cmd = |stake: i64| play_sub("shells", vec![int_option("bet", stake)]);
    let off = sim.command(player, "play", shells_cmd(20)).await;
    assert!(
        off.ephemeral && off.content().contains("**kapalı**"),
        "{}",
        off.content()
    );
    gambling(&mut sim, manager, true).await;
    let broke = sim.command(unique(), "play", shells_cmd(20)).await;
    assert!(
        broke.ephemeral && broke.content().contains("Yalnızca **0** kabuğun var"),
        "{}",
        broke.content()
    );

    // A right guess pays two and a half times the stake.
    let table = sim.command(player, "play", shells_cmd(20)).await;
    assert!(
        !table.ephemeral && table.content().contains("kabuk masasına **20** kabukla oturdu"),
        "{}",
        table.content()
    );
    assert_eq!(table.title(), "Kabuk oyunu");
    assert_png(&table, 520, 320);
    keep_png(&sim, &table, "shells-tr-table");
    assert_eq!(table.controls().count(), 3);
    for label in ["1. kabuk", "2. kabuk", "3. kabuk"] {
        assert!(table.has_button(label), "{label}");
    }
    let session = session_of(&table);
    let pearl = pearl_under(&sim, session).await;
    let stranger = sim.click(unique(), &table, &format!("{}. kabuk", pearl + 1)).await;
    assert!(
        stranger.ephemeral && stranger.content().contains("senin masan değil"),
        "{}",
        stranger.content()
    );
    let found = sim.click(player, &table, &format!("{}. kabuk", pearl + 1)).await;
    assert!(
        found.components.is_empty()
            && found
                .content()
                .contains(&format!("inciyi {}. kabuğun altında buldu", pearl + 1))
            && found.content().contains("**50** kabuk kazandı"),
        "{}",
        found.content()
    );
    keep_png(&sim, &found, "shells-tr-found");
    let over = sim.click(player, &table, &format!("{}. kabuk", pearl + 1)).await;
    assert!(over.ephemeral && over.content().contains("bitti"), "{}", over.content());

    // A wrong guess says where the pearl was.
    let table = sim.command(loser, "play", shells_cmd(20)).await;
    let session = session_of(&table);
    let pearl = pearl_under(&sim, session).await;
    let wrong = (pearl + 1) % 3;
    sim.app
        .cache
        .forget(&format!("rest:shellsstart:{loser}"))
        .await
        .unwrap();
    let missed = sim.click(loser, &table, &format!("{}. kabuk", wrong + 1)).await;
    assert!(
        missed.content().contains(&format!("{}. kabuğu seçti", wrong + 1))
            && missed.content().contains(&format!("{}. kabuğun altındaydı", pearl + 1)),
        "{}",
        missed.content()
    );
    keep_png(&sim, &missed, "shells-tr-missed");

    // Nobody picks: the janitor gives the stake back, in the player's own language.
    sim.app
        .cache
        .forget(&format!("rest:shellsstart:{loser}"))
        .await
        .unwrap();
    sim.app
        .set_user_lang(loser, Some(pinchora_core::i18n::Lang::Tr))
        .await
        .unwrap();
    sim.locale = "en-US";
    let table = sim.command(loser, "play", shells_cmd(20)).await;
    let session = session_of(&table);
    let message = give_message(&sim, session).await;
    expire(&sim, session).await;
    sweep_games(&sim.app).await.expect("sweep");
    let posts = sim.posts();
    let post = posts
        .iter()
        .find(|p| p.message == message)
        .expect("the table was edited");
    assert!(
        post.content().contains("hiç kabuk seçmedi") && post.content().contains("geri döndü"),
        "{}",
        post.content()
    );
    sim.app.set_user_lang(loser, None).await.unwrap();
}

#[tokio::test]
async fn a_turkish_storm_surge_is_called_stacked_and_lost_in_turkish() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) = Sim::new("a_turkish_storm_surge_is_called_stacked_and_lost_in_turkish").await else {
        return;
    };
    forget_late_games(&sim).await;
    sim.channel = unique();
    sim.locale = "tr";
    turkish_server(&sim).await;
    let (manager, member) = (unique(), unique());
    let denied = sim.command(member, "surge", json!([])).await;
    assert!(
        denied.ephemeral && denied.content().contains("Sunucuyu Yönet"),
        "{}",
        denied.content()
    );
    let storm = call_storm(&mut sim, manager).await;
    assert!(
        !storm.ephemeral && storm.content().contains("Fırtına kabarması!") && storm.has_button("Kum torbası yığ"),
        "{}",
        storm.content()
    );
    assert_eq!(storm.title(), "Fırtına Kabarması");
    assert!(storm.pings_nobody());
    assert_png(&storm, 640, 380);
    keep_png(&sim, &storm, "surge-tr-rolling");
    let session = session_of(&storm);
    let state = sim.app.db.game(session).await.unwrap().unwrap().state;
    assert!(state.ends_with("|tr"), "{state}");
    let again = call_storm(&mut sim, manager).await;
    assert!(
        again.ephemeral && again.content().contains("nefes alması lazım"),
        "{}",
        again.content()
    );
    sim.app
        .cache
        .forget(&format!("rest:surge:{}", sim.guild))
        .await
        .unwrap();
    let raging = call_storm(&mut sim, manager).await;
    assert!(
        raging.ephemeral && raging.content().contains("zaten bir fırtına kopuyor"),
        "{}",
        raging.content()
    );
    let helper = unique();
    let first = sim.click(helper, &storm, "Kum torbası yığ").await;
    assert!(
        first.ephemeral && first.content().contains("**1.** kum torbası yığıldı"),
        "{}",
        first.content()
    );
    let eager = sim.click(helper, &storm, "Kum torbası yığ").await;
    assert!(
        eager.ephemeral && eager.content().contains("Kıskaçlarının dinlenmesi lazım"),
        "{}",
        eager.content()
    );

    // Nobody holds the wall: the sea wins, and the janitor says so in the storm's language.
    let message = give_message(&sim, session).await;
    sim.locale = "en-US";
    expire(&sim, session).await;
    sweep_games(&sim.app).await.expect("sweep");
    let posts = sim.posts();
    let edit = posts
        .iter()
        .find(|p| p.message == message)
        .expect("the storm was edited");
    assert!(
        edit.content().contains("Deniz duvarı aştı") && !edit.content().contains("sea broke"),
        "{}",
        edit.content()
    );
    let (_, png) = edit.files.first().expect("the card");
    std::fs::write(sim.dir.join("surge-tr-broken.png"), png).expect("save the picture");
    let late = sim.click(helper, &storm, "Kum torbası yığ").await;
    assert!(late.ephemeral, "{}", late.content());
}

#[tokio::test]
async fn timed_turkish_storms_are_configured_in_turkish() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) = Sim::new("timed_turkish_storms_are_configured_in_turkish").await else {
        return;
    };
    forget_late_games(&sim).await;
    sim.locale = "tr";
    let (manager, helper, lounge) = (unique(), unique(), unique());
    sim.permissions = MANAGE_GUILD;
    let config = |options: Vec<Value>| json!([sub("surge", options)]);
    let off = sim.command(manager, "config", config(vec![])).await;
    assert!(
        off.ephemeral && off.content().contains("**kapalı**"),
        "{}",
        off.content()
    );
    let needs = sim
        .command(manager, "config", config(vec![bool_option("enabled", true)]))
        .await;
    assert!(needs.content().contains("bir kanal gerekiyor"), "{}", needs.content());
    let on = sim
        .command_resolved(
            manager,
            "config",
            config(vec![channel_option("channel", lounge), int_option("every", 6)]),
            resolved_channels(&[lounge]),
        )
        .await;
    assert!(
        on.content().contains(&format!("<#{lounge}>")) && on.content().contains("her **6** saatte bir"),
        "{}",
        on.content()
    );
    sim.permissions = 0;
    let refused = sim
        .command(helper, "config", config(vec![bool_option("enabled", false)]))
        .await;
    assert!(refused.content().contains("Sunucuyu Yönet"), "{}", refused.content());

    // The storm that comes by itself speaks the server's language (here: set to Turkish for the occasion).
    sim.app
        .set_guild_lang(sim.guild, Some(pinchora_core::i18n::Lang::Tr))
        .await
        .unwrap();
    sim.app.shift_clock(chrono::Duration::hours(7));
    sweep_games(&sim.app).await.expect("sweep");
    let posts = sim.posts();
    let storm = posts
        .iter()
        .find(|p| p.channel == lounge && p.method == Method::POST)
        .expect("the storm came to the channel");
    assert!(storm.content().contains("Fırtına kabarması!"), "{}", storm.content());

    // Clean up so no stale slot is left in the database.
    sim.app.set_guild_lang(sim.guild, None).await.unwrap();
    sim.permissions = MANAGE_GUILD;
    sim.locale = "en-US";
    let off = sim
        .command(manager, "config", config(vec![bool_option("enabled", false)]))
        .await;
    assert!(off.content().contains("**off**"), "{}", off.content());
    let running = sim.app.db.running_surges(1_000).await.unwrap();
    if let Some(mine) = running.iter().find(|s| s.guild_id == sim.guild as i64) {
        expire(&sim, mine.id).await;
    }
    sweep_games(&sim.app).await.expect("sweep");
}

#[tokio::test]
async fn play_stats_speak_turkish_with_the_games_turkish_names() {
    let Some(mut sim) = Sim::new("play_stats_speak_turkish_with_the_games_turkish_names").await else {
        return;
    };
    let player = unique();
    sim.locale = "tr";
    let empty = sim.command(player, "play", play_sub("stats", vec![])).await;
    assert!(
        empty.description().contains("Henüz oyun yok") && empty.title().ends_with("kullanıcısının oyunları"),
        "{} / {}",
        empty.title(),
        empty.description()
    );
    assert_eq!(empty.field("❌ XOX"), "Henüz oynanmadı");
    assert_eq!(empty.field("🔴 Dört Bağla"), "Henüz oynanmadı");
    assert_eq!(empty.field("⚪ İnci Dama"), "Henüz oynanmadı");

    sim.app.db.record_game(player, "connect4", true).await.unwrap();
    sim.app.db.record_game(player, "connect4", false).await.unwrap();
    let played = sim.command(player, "play", play_sub("stats", vec![])).await;
    assert!(
        played.description().contains("**2** oyunda **1** galibiyet"),
        "{}",
        played.description()
    );
    assert!(
        played
            .field("🔴 Dört Bağla")
            .contains("**2** oyunun **1** tanesi galibiyet")
            && played.field("🔴 Dört Bağla").contains("%50 galibiyet oranı"),
        "{}",
        played.field("🔴 Dört Bağla")
    );
    let robo = json!({ "users": { "77": person_json(77, "robo", Some("Robo"), true) } });
    let bot = sim
        .command_resolved(player, "play", play_sub("stats", vec![user_option("user", 77)]), robo)
        .await;
    assert!(
        bot.ephemeral && bot.content().contains("hakemlik yapar"),
        "{}",
        bot.content()
    );
}

#[tokio::test]
async fn a_lobby_or_table_is_in_the_servers_language_whoever_opens_or_joins_it() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) = Sim::new("a_lobby_or_table_is_in_the_servers_language_whoever_opens_or_joins_it").await else {
        return;
    };
    forget_late_games(&sim).await;
    sim.channel = unique();
    let (host, joiner, manager, player, stranger) = (unique(), unique(), unique(), unique(), unique());
    fund(&sim, player, 200).await;
    gambling(&mut sim, manager, true).await;
    let names = ROSTER.map(|c| c.name);
    let shells_cmd = || play_sub("shells", vec![int_option("bet", 20)]);

    // A Turkish server, English-speaking players: every public message is Turkish and says so in its state.
    turkish_server(&sim).await;
    sim.locale = "en-US";
    let derby = open_derby(&mut sim, host, None).await;
    assert_eq!(derby.title(), "Keşiş Yengeç Yarışı");
    let state = sim.app.db.game(session_of(&derby)).await.unwrap().unwrap().state;
    assert!(state.ends_with("|tr"), "{state}");
    let joined = sim.click(joiner, &derby, names[0]).await;
    assert_eq!(
        joined.title(),
        "Keşiş Yengeç Yarışı",
        "an English click keeps the lobby Turkish"
    );
    let heist = open_heist(&mut sim, host).await;
    assert_eq!(heist.title(), "Hazine Soygunu");
    let state = sim.app.db.game(session_of(&heist)).await.unwrap().unwrap().state;
    assert!(state.ends_with("|tr"), "{state}");
    let storm = call_storm(&mut sim, manager).await;
    assert_eq!(storm.title(), "Fırtına Kabarması");
    let table = sim.command(player, "play", shells_cmd()).await;
    assert_eq!(table.title(), "Kabuk oyunu");
    let session = session_of(&table);
    let state = sim.app.db.game(session).await.unwrap().unwrap().state;
    assert!(state.ends_with("|tr"), "the table's language is stored: {state}");
    // What only the clicker sees is theirs, the table's own end is the server's.
    let refused = sim.click(stranger, &table, "2. kabuk").await;
    assert!(
        refused.ephemeral && refused.content().contains("isn't your table"),
        "{}",
        refused.content()
    );
    let pearl = pearl_under(&sim, session).await;
    let guessed = sim.click(player, &table, &format!("{}. kabuk", pearl + 1)).await;
    assert!(
        guessed.content().contains("inci") && !guessed.content().contains("pearl"),
        "{}",
        guessed.content()
    );
    assert_eq!(guessed.title(), "Kabuk oyunu");

    // An English server, Turkish-speaking players: the tables stay English.
    sim.app
        .set_guild_lang(sim.guild, None)
        .await
        .expect("clear the server's language");
    sim.channel = unique();
    sim.locale = "tr";
    let derby = open_derby(&mut sim, unique(), None).await;
    assert_eq!(derby.title(), "Hermit Crab Derby");
    let state = sim.app.db.game(session_of(&derby)).await.unwrap().unwrap().state;
    assert!(state.ends_with("|en"), "{state}");
    let heist = open_heist(&mut sim, unique()).await;
    assert_eq!(heist.title(), "Treasure Heist");
    fund(&sim, joiner, 100).await;
    let table = sim.command(joiner, "play", shells_cmd()).await;
    assert_eq!(table.title(), "Shell game");
    let refused = sim.click(stranger, &table, "Shell 2").await;
    assert!(
        refused.ephemeral && refused.content().contains("senin masan değil"),
        "{}",
        refused.content()
    );
}
