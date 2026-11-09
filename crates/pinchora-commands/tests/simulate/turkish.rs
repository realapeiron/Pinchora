//! The word and quiz games in Turkish: a player whose Discord is set to Turkish gets Turkish content, the
//! Turkish alphabet and keyboard, and a daily wordle of their own.

use pinchora_core::infra::i18n::Lang;
use pinchora_game::{
    lang,
    play::{find_question, hangman::Hangman, trivia, wordle},
};

use crate::*;

#[tokio::test]
async fn a_turkish_player_gets_the_turkish_daily_wordle_with_the_turkish_keyboard() {
    let Some(mut sim) = Sim::new("a_turkish_player_gets_the_turkish_daily_wordle_with_the_turkish_keyboard").await
    else {
        return;
    };
    let player = unique();
    let day = today();
    let words = &PlayContent::for_lang(Lang::Tr).words;
    let answer = wordle::daily_word(&words.answers, day).to_owned();
    sim.locale = "tr";

    let board = sim.command(player, "play", json!([sub("wordle", vec![])])).await;
    assert!(board.ephemeral);
    assert_eq!((board.files.len(), board.files[0].0.as_str()), (1, "wordle.png"));
    assert!(board.title().starts_with("Günlük wordle #"), "{}", board.title());
    assert!(
        board.description().contains("Beş harfli kelimeyi"),
        "{}",
        board.description()
    );

    let form = sim.click(player, &board, "Tahmin et").await;
    assert_eq!(
        form.modal.as_ref().map(|m| m.0.clone()),
        Some(format!("wo:m:daily-tr:{day}")),
        "{form}"
    );
    // Letters Turkish words never use are refused; so is a word that is not on the list.
    let foreign = sim.submit(player, &form, "quark", &board).await;
    assert!(
        foreign.ephemeral && foreign.content().contains("beş harf"),
        "{}",
        foreign.content()
    );
    let nonsense = sim.submit(player, &form, "zzzzz", &board).await;
    assert!(nonsense.content().contains("listemde yok"), "{}", nonsense.content());

    // A miss that has Turkish letters in it, typed in capitals the way a Turkish keyboard makes them.
    let miss = words
        .allowed
        .iter()
        .find(|w| **w != answer && w.contains('ı') && w.contains('ş') && w.chars().any(|c| "çğöü".contains(c)))
        .or_else(|| {
            words
                .allowed
                .iter()
                .find(|w| **w != answer && w.contains('ı') && w.contains('ş'))
        })
        .expect("an allowed word with Turkish letters")
        .clone();
    let board = sim.submit(player, &form, &Lang::Tr.upper(&miss), &board).await;
    assert!(board.description().contains("tahmin et"), "{}", board.description());
    keep_png(&sim, &board, "wordle-tr-one-guess");
    let stored = sim
        .app
        .db
        .wordle_game(player, "daily-tr", day as i32)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (stored.guesses.clone(), stored.status.as_str()),
        (vec![miss], "playing")
    );
    assert!(
        sim.app
            .db
            .wordle_game(player, "daily", day as i32)
            .await
            .unwrap()
            .is_none(),
        "the English daily is a board of its own"
    );

    let won = sim.submit(player, &form, &answer, &board).await;
    assert!(
        won.description().contains("**2** denemede buldun") && won.description().contains("+**40** kabuk"),
        "{}",
        won.description()
    );
    keep_png(&sim, &won, "wordle-tr-won");
    assert_eq!(shells(&sim, player).await, 40);
    assert_eq!(record_of(&sim, player, "wordle").await, (1, 1));

    let shared = sim.click(player, &won, "Sonucu paylaş").await;
    assert!(!shared.ephemeral, "{}", shared.content());
    assert!(
        shared.content().contains("Pinchora Wordle TR #"),
        "{}",
        shared.content()
    );
    assert!(shared.content().contains("2/6") && !shared.content().contains(&answer));

    // The English daily is another word, and the day's reward is only paid once.
    sim.locale = "en-US";
    let english_answer = wordle::daily_word(&PlayContent::get().words.answers, day).to_owned();
    let board = sim.command(player, "play", json!([sub("wordle", vec![])])).await;
    assert!(board.title().starts_with("Daily wordle #"), "{}", board.title());
    let form = sim.click(player, &board, "Guess").await;
    assert_eq!(
        form.modal.as_ref().map(|m| m.0.clone()),
        Some(format!("wo:m:daily:{day}"))
    );
    let won = sim.submit(player, &form, &english_answer, &board).await;
    assert!(
        won.description().contains("You got it in **1**") && won.description().contains("other language"),
        "{}",
        won.description()
    );
    assert_eq!(shells(&sim, player).await, 40, "no second daily reward");
}

#[tokio::test]
async fn turkish_practice_wordle_resumes_and_loses_with_the_word_shown() {
    let Some(mut sim) = Sim::new("turkish_practice_wordle_resumes_and_loses_with_the_word_shown").await else {
        return;
    };
    let player = unique();
    sim.locale = "tr";
    let words = &PlayContent::for_lang(Lang::Tr).words;
    let board = sim
        .command(
            player,
            "play",
            json!([sub("wordle", vec![text_option("mode", "practice")])]),
        )
        .await;
    assert_eq!(board.title(), "Serbest wordle");
    let first = sim
        .app
        .db
        .wordle_game(player, "practice-tr", 0)
        .await
        .unwrap()
        .expect("a Turkish practice board");
    assert!(words.answers.contains(&first.word));
    let form = sim.click(player, &board, "Tahmin et").await;
    let mut board = board;
    let misses: Vec<&String> = words
        .allowed
        .iter()
        .filter(|w| **w != first.word && w.contains('ğ') || w.contains('ç'))
        .filter(|w| **w != first.word)
        .take(6)
        .collect();
    for miss in &misses {
        board = sim.submit(player, &form, miss, &board).await;
    }
    assert!(board.description().contains("Hakkın bitti"), "{}", board.description());
    assert!(
        board.description().contains(&Lang::Tr.upper(&first.word)),
        "{}",
        board.description()
    );
    keep_png(&sim, &board, "wordle-tr-lost");
    let fresh = sim.click(player, &board, "Yeni kelime").await;
    assert_eq!(fresh.title(), "Serbest wordle");
}

#[tokio::test]
async fn turkish_hangman_plays_with_all_twenty_nine_letters() {
    let Some(mut sim) = Sim::new("turkish_hangman_plays_with_all_twenty_nine_letters").await else {
        return;
    };
    let player = unique();
    sim.locale = "tr";
    let start = sim.command(player, "play", json!([sub("hangman", vec![])])).await;
    assert_eq!((start.files.len(), start.files[0].0.as_str()), (1, "hangman.png"));
    assert!(
        start.content().contains("kum kalesini kurtarıyor"),
        "{}",
        start.content()
    );
    assert_eq!(start.rows(), 2, "two letter menus");
    let id = session_of(&start);
    let state = sim.app.db.game(id).await.unwrap().unwrap().state;
    assert!(state.ends_with("|tr"), "{state}");
    let game = Hangman::decode(&state).expect("a Turkish hangman state");
    assert_eq!(game.lang(), Lang::Tr);
    let word = game.word().to_owned();
    assert!(PlayContent::for_lang(Lang::Tr).hangman.iter().any(|h| h.word == word));

    let first: Vec<_> = start
        .options(&format!("hm:{id}:a"))
        .iter()
        .map(|o| o.label.as_str())
        .collect();
    let second: Vec<_> = start
        .options(&format!("hm:{id}:n"))
        .iter()
        .map(|o| o.label.as_str())
        .collect();
    assert_eq!((first.len(), second.len()), (15, 14));
    for letter in ["Ç", "Ğ", "İ", "I"] {
        assert!(first.contains(&letter), "{letter} is in the first menu: {first:?}");
    }
    for letter in ["Ö", "Ş", "Ü"] {
        assert!(second.contains(&letter), "{letter} is in the second menu: {second:?}");
    }
    assert!(!first.iter().chain(&second).any(|l| ["Q", "W", "X"].contains(l)));

    let half = |c: char| {
        if lang::TR_ALPHABET.chars().take(15).any(|a| a == c) {
            "a"
        } else {
            "n"
        }
    };
    // One miss first, then every letter of the word.
    let wrong = lang::TR_ALPHABET
        .chars()
        .find(|c| !word.contains(*c))
        .expect("a letter not in the word");
    let mut board = sim
        .pick(player, &start, &format!("hm:{id}:{}", half(wrong)), &wrong.to_string())
        .await;
    keep_png(&sim, &board, "hangman-tr-one-miss");
    let mut letters: Vec<char> = word.chars().filter(|c| *c != ' ').collect();
    letters.sort_unstable();
    letters.dedup();
    for letter in letters {
        board = sim
            .pick(
                player,
                &board,
                &format!("hm:{id}:{}", half(letter)),
                &letter.to_string(),
            )
            .await;
    }
    assert!(
        board.content().contains("kum kalesini kurtardı") && board.content().contains("+**15**"),
        "{}",
        board.content()
    );
    keep_png(&sim, &board, "hangman-tr-won");
    assert_eq!(record_of(&sim, player, "hangman").await, (1, 1));
}

#[tokio::test]
async fn a_turkish_trivia_round_asks_a_turkish_question() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) = Sim::new("a_turkish_trivia_round_asks_a_turkish_question").await else {
        return;
    };
    forget_late_games(&sim).await;
    sim.channel = unique();
    sim.locale = "tr";
    let (starter, right) = (unique(), unique());
    let round = sim
        .command(
            starter,
            "play",
            json!([sub("trivia", vec![text_option("category", "ocean")])]),
        )
        .await;
    assert!(round.title().contains("Okyanus"), "{}", round.title());
    assert!(round.content().contains("bilgi yarışması turu"), "{}", round.content());
    assert_eq!(round.controls().count(), 4, "four answers");
    let id = session_of(&round);
    let state = sim.app.db.game(id).await.unwrap().unwrap().state;
    let (question_id, order) = state.split_once(':').expect("question:order");
    let question = find_question(question_id).expect("a real question");
    assert_eq!(question.lang, Lang::Tr);
    assert!(question_id.contains("_tr_"), "{question_id}");
    assert!(
        PlayContent::get().question(question_id).is_none(),
        "not an English question"
    );
    let order = trivia::decode_order(order).expect("an order");
    for slot in 0..4 {
        assert!(
            round.description().contains(question.shown(order)[slot]),
            "option {slot} is shown"
        );
    }
    assert!(round.description().contains(&question.question));
    let correct = question.correct_slot(order).expect("the answer is shown");
    let locked = sim.component(right, &format!("tv:{id}:{correct}"), &[], &round).await;
    assert!(locked.ephemeral, "{}", locked.content());
    // Another round, in English, still draws from the English questions.
    sim.locale = "en-US";
    sim.channel = unique();
    let english = sim
        .command(
            unique(),
            "play",
            json!([sub("trivia", vec![text_option("category", "ocean")])]),
        )
        .await;
    assert!(english.title().contains("Ocean"), "{}", english.title());
}

/// Keeps the picture of a reply next to the sim's own, under a name that says what it shows.
fn keep_png(sim: &Sim, reply: &Reply, name: &str) {
    let (_, png) = reply.files.first().expect("the reply carried a picture");
    std::fs::write(sim.dir.join(format!("{name}.png")), png).expect("save the picture");
}
