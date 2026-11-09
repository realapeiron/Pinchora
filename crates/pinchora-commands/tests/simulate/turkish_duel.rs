//! The two-player duels in Turkish: challenges, accepting, moves, endings, time-outs and refusals for
//! tic-tac-toe, Connect four, rock paper scissors, Share or Grab, Sea Fleet, Pearl Checkers and Reef Chess.
//! What only the clicker sees (refusals, menus, a private picker) is in the language of whoever clicked; what the
//! whole channel reads (a challenge, a board, a time-out) stays in the server's language for its whole life,
//! whoever clicked it. Pictures land in `target/sim/<test>/`.

use pinchora_core::infra::i18n::Lang;
use pinchora_game::play::fleet;
use pinchora_game::play::fleet::Fleet;

use crate::*;

/// A bot the duels turn away.
const BOT: u64 = 55;

/// Keeps the picture of a reply next to the sim's own, under a name that says what it shows.
fn keep_png(sim: &Sim, reply: &Reply, name: &str) {
    let (_, png) = reply.files.first().expect("the reply carried a picture");
    std::fs::write(sim.dir.join(format!("{name}.png")), png).expect("save the picture");
}

/// The same for a message the bot edited by itself.
fn keep_post_png(sim: &Sim, post: &Post, name: &str) {
    let (_, png) = post.files.first().expect("the edit carried a picture");
    std::fs::write(sim.dir.join(format!("{name}.png")), png).expect("save the picture");
}

/// How Discord writes a mention.
fn mention(user: u64) -> String {
    format!("<@{user}>")
}

/// All the text a player reads in a reply: the message, the embed, the buttons and the menus.
fn everything(reply: &Reply) -> String {
    let mut all = String::from(reply.content());
    for embed in &reply.embeds {
        all.push('\n');
        all.push_str(embed.title.as_deref().unwrap_or_default());
        all.push('\n');
        all.push_str(embed.description.as_deref().unwrap_or_default());
        if let Some(footer) = &embed.footer {
            all.push('\n');
            all.push_str(&footer.text);
        }
    }
    for control in reply.controls() {
        match control {
            Component::Button(button) => {
                all.push('\n');
                all.push_str(button.label.as_deref().unwrap_or_default());
            }
            Component::SelectMenu(menu) => {
                all.push('\n');
                all.push_str(menu.placeholder.as_deref().unwrap_or_default());
                for option in menu.options.as_deref().unwrap_or_default() {
                    all.push('\n');
                    all.push_str(&option.label);
                    all.push('\n');
                    all.push_str(option.description.as_deref().unwrap_or_default());
                }
            }
            _ => {}
        }
    }
    all
}

/// Fails when a catalog key (`duel.pot-line`) reached the player instead of its text: a key that is missing
/// from every language is printed as itself.
fn assert_no_keys(text: &str) {
    for prefix in [
        "duel",
        "tictactoe",
        "connect4",
        "rps",
        "share",
        "seafleet",
        "checkers",
        "chess",
        "board",
    ] {
        let needle = format!("{prefix}.");
        let mut rest = text;
        while let Some(at) = rest.find(&needle) {
            let before = rest[..at].chars().next_back();
            let after = rest[at + needle.len()..].chars().next();
            assert!(
                before.is_some_and(char::is_alphanumeric) || !after.is_some_and(|c| c.is_ascii_lowercase()),
                "a catalog key reached the player: {needle}{} in {text}",
                after.unwrap_or(' ')
            );
            rest = &rest[at + needle.len()..];
        }
    }
}

/// `/duel <game>` that also checks the answer for catalog keys.
async fn invite(sim: &mut Sim, host: u64, guest: u64, game: &str, extra: Vec<Value>) -> Reply {
    let reply = duel(sim, host, guest, game, extra).await;
    assert_no_keys(&everything(&reply));
    reply
}

/// [`Sim::click`] that also checks the answer for catalog keys.
async fn click(sim: &mut Sim, user: u64, on: &Reply, label: &str) -> Reply {
    let reply = sim.click(user, on, label).await;
    assert_no_keys(&everything(&reply));
    reply
}

/// [`Sim::pick`] that also checks the answer for catalog keys.
async fn pick(sim: &mut Sim, user: u64, on: &Reply, menu: &str, value: &str) -> Reply {
    let reply = sim.pick(user, on, menu, value).await;
    assert_no_keys(&everything(&reply));
    reply
}

/// Who is to move in a Turkish duel message that says `Sıra: <@id>`.
fn mover(reply: &Reply, a: u64, b: u64) -> u64 {
    let content = reply.content();
    if content.contains(&format!("Sıra: {}", mention(a))) {
        a
    } else if content.contains(&format!("Sıra: {}", mention(b))) {
        b
    } else {
        panic!("nobody is to move: {content}")
    }
}

/// A message rebuilt from what the bot edited in by itself (a post), so a test can go on clicking it.
fn reply_of(post: &Post) -> Reply {
    use twilight_model::http::interaction::InteractionResponseType;
    let mut reply = Reply::new(InteractionResponseType::UpdateMessage);
    reply.message_id = post.message;
    let embeds = serde_json::from_value(post.body["embeds"].clone()).ok();
    let components = serde_json::from_value::<Vec<Component>>(post.body["components"].clone()).ok();
    reply.edit(post.body["content"].as_str().map(str::to_owned), embeds, components);
    reply.files = post.files.clone();
    reply
}

/// The newest message the bot edited by itself.
fn latest_post(sim: &Sim) -> Post {
    sim.posts().into_iter().next_back().expect("the message was edited")
}

/// Sets a session's own fields, keeping the names that ride in its state.
async fn set_fields(sim: &Sim, session: i64, fields: &str) {
    let (state,): (String,) = sqlx::query_as("SELECT state FROM game_sessions WHERE id = $1")
        .bind(session)
        .fetch_one(sim.app.db.pool())
        .await
        .expect("the session");
    let names = state.split_once('|').map_or("", |(_, names)| names);
    sqlx::query("UPDATE game_sessions SET state = $2 WHERE id = $1")
        .bind(session)
        .bind(format!("{fields}|{names}"))
        .execute(sim.app.db.pool())
        .await
        .expect("set the state");
}

// /Tic-tac-toe

#[tokio::test]
async fn tic_tac_toe_in_turkish() {
    let Some(mut sim) = Sim::new("tic_tac_toe_in_turkish").await else {
        return;
    };
    sim.locale = "tr";
    turkish_server(&sim).await;
    let (x, o, bystander, rival) = (unique(), unique(), unique(), unique());

    // A bot is no opponent, in any of the duels.
    let robo = json!({ "users": { BOT.to_string(): person_json(BOT, "robo", Some("Robo"), true) } });
    for game in ["tictactoe", "connect4", "rps", "share", "seafleet", "checkers", "chess"] {
        let refused = sim
            .command_resolved(
                x,
                "duel",
                json!([sub(game, vec![user_option("user", BOT)])]),
                robo.clone(),
            )
            .await;
        assert!(
            refused.ephemeral && refused.content().contains("Bir insan rakip seç"),
            "{game}: {}",
            refused.content()
        );
    }

    let mut game = invite(&mut sim, x, o, "tictactoe", vec![]).await;
    assert!(
        game.content().contains("İlk hamle") && game.content().contains(&mention(x)),
        "{}",
        game.content()
    );
    let id = |game: &Reply, cell: usize| match game.controls().nth(cell) {
        Some(Component::Button(b)) => b.custom_id.clone().expect("custom id"),
        other => panic!("cell {cell} is {other:?}"),
    };
    // O is not on turn, and a stranger is not in the game.
    let early = sim.component(o, &id(&game, 4), &[], &game).await;
    assert!(
        early.ephemeral && early.content().contains("Sıra henüz sende değil"),
        "{}",
        early.content()
    );
    let outsider = sim.component(bystander, &id(&game, 4), &[], &game).await;
    assert!(
        outsider.ephemeral && outsider.content().contains("Bu oyunda değilsin"),
        "{}",
        outsider.content()
    );
    game = sim.component(x, &id(&game, 0), &[], &game).await;
    assert!(
        game.content().contains(&format!("Sıra: {} (⭕)", mention(o))),
        "{}",
        game.content()
    );
    let taken = sim.component(o, &id(&game, 0), &[], &game).await;
    assert!(
        taken.ephemeral && taken.content().contains("Bu kare dolu"),
        "{}",
        taken.content()
    );
    for (player, cell) in [(o, 3), (x, 1), (o, 4), (x, 2)] {
        game = sim.component(player, &id(&game, cell), &[], &game).await;
    }
    assert!(
        game.content().contains("kazandı ve **30** kabuk aldı"),
        "{}",
        game.content()
    );
    assert_eq!(shells(&sim, x).await, 30);

    // The same pair is paid once an hour.
    let mut again = invite(&mut sim, o, x, "tictactoe", vec![]).await;
    for (player, cell) in [(o, 0), (x, 3), (o, 1), (x, 4), (o, 2)] {
        again = sim.component(player, &id(&again, cell), &[], &again).await;
    }
    assert!(again.content().contains("saatte bir sıfırlanır"), "{}", again.content());

    // A full board with no line is a draw.
    let mut draw = invite(&mut sim, x, rival, "tictactoe", vec![]).await;
    for (player, cell) in [
        (x, 0),
        (rival, 1),
        (x, 2),
        (rival, 4),
        (x, 3),
        (rival, 5),
        (x, 7),
        (rival, 6),
        (x, 8),
    ] {
        draw = sim.component(player, &id(&draw, cell), &[], &draw).await;
    }
    assert!(draw.content().contains("Berabere!"), "{}", draw.content());
    assert_eq!(record_of(&sim, x, "tictactoe").await.0, 3);
}

// /Connect four and the challenge flow

#[tokio::test]
async fn connect_four_in_turkish_from_challenge_to_win() {
    let Some(mut sim) = Sim::new("connect_four_in_turkish_from_challenge_to_win").await else {
        return;
    };
    sim.locale = "tr";
    turkish_server(&sim).await;
    let (a, b, bystander) = (unique(), unique(), unique());

    let challenge = invite(&mut sim, a, b, "connect4", vec![]).await;
    assert!(
        !challenge.ephemeral
            && challenge.content().contains("meydan okuyor")
            && challenge.content().contains("**Dört Bağla**"),
        "{}",
        challenge.content()
    );
    assert!(challenge.content().contains("Bahis yok"), "{}", challenge.content());
    assert!(
        challenge.has_button("Kabul et") && challenge.has_button("Reddet"),
        "{}",
        everything(&challenge)
    );
    let refused = click(&mut sim, a, &challenge, "Kabul et").await;
    assert!(
        refused.ephemeral && refused.content().contains("Bu meydan okumayı sen gönderdin"),
        "{}",
        refused.content()
    );
    let refused = click(&mut sim, bystander, &challenge, "Kabul et").await;
    assert!(
        refused.ephemeral && refused.content().contains("Bu meydan okuma sana değil"),
        "{}",
        refused.content()
    );
    let refused = click(&mut sim, bystander, &challenge, "Reddet").await;
    assert!(
        refused.ephemeral && refused.content().contains("senin değil"),
        "{}",
        refused.content()
    );

    let board = click(&mut sim, b, &challenge, "Kabul et").await;
    assert_eq!((board.files.len(), board.files[0].0.as_str()), (1, "connect4.png"));
    assert_eq!(board.title(), "Dört Bağla");
    assert!(
        board.content().contains(&format!("Sıra: {}", mention(a))) && board.content().contains("hamle yapmalı"),
        "{}",
        board.content()
    );
    keep_png(&sim, &board, "connect4-tr-start");
    let refused = click(&mut sim, b, &board, "3").await;
    assert!(
        refused.ephemeral && refused.content().contains("Sıra henüz sende değil"),
        "{}",
        refused.content()
    );
    let refused = click(&mut sim, bystander, &board, "3").await;
    assert!(
        refused.ephemeral && refused.content().contains("Bu oyunda değilsin"),
        "{}",
        refused.content()
    );

    // A stacks column 1 while B answers in column 2: A's fourth disc wins.
    let mut board = board;
    for (player, label) in [(a, "1"), (b, "2"), (a, "1"), (b, "2"), (a, "1"), (b, "2")] {
        board = click(&mut sim, player, &board, label).await;
    }
    keep_png(&sim, &board, "connect4-tr-midgame");
    let won = click(&mut sim, a, &board, "1").await;
    assert!(
        won.content().contains(&format!("{} kazandı!", mention(a))) && won.content().contains("+**25** kabuk!"),
        "{}",
        won.content()
    );
    assert!(won.components.is_empty() && won.files.len() == 1);
    keep_png(&sim, &won, "connect4-tr-won");
    let late = sim
        .component(b, &board.button("2").custom_id.clone().expect("id"), &[], &board)
        .await;
    assert!(
        late.ephemeral && late.content().contains("Bu oyun bitti"),
        "{}",
        late.content()
    );
    assert_eq!((shells(&sim, a).await, shells(&sim, b).await), (25, 0));
}

#[tokio::test]
async fn challenges_and_wagers_in_turkish_with_their_refusals() {
    let Some(mut sim) = Sim::new("challenges_and_wagers_in_turkish_with_their_refusals").await else {
        return;
    };
    sim.locale = "tr";
    turkish_server(&sim).await;
    let (a, b, broke, rich) = (unique(), unique(), unique(), unique());
    fund(&sim, a, 500).await;
    fund(&sim, b, 500).await;
    fund(&sim, rich, 500).await;
    fund(&sim, broke, 30).await;

    // A challenge holds nothing; a player who cannot cover it cannot accept, and nothing is taken.
    let first = invite(&mut sim, rich, broke, "connect4", vec![int_option("bet", 100)]).await;
    assert!(
        first.content().contains("Bahis: kişi başı **100** kabuk") && first.content().contains("hükmen yenilir"),
        "{}",
        first.content()
    );
    // Another challenge so soon is turned away.
    let hasty = invite(&mut sim, rich, a, "connect4", vec![]).await;
    assert!(
        hasty.ephemeral && hasty.content().contains("Yavaş ol!") && hasty.content().contains("sn sonra"),
        "{}",
        hasty.content()
    );
    let refused = click(&mut sim, broke, &first, "Kabul et").await;
    assert!(
        refused.ephemeral
            && refused
                .content()
                .contains("Yalnızca **30** kabuğun var, ama bahis kişi başı **100**"),
        "{}",
        refused.content()
    );
    let declined = click(&mut sim, broke, &first, "Reddet").await;
    assert!(
        declined.content().contains("meydan okumayı reddetti") && declined.components.is_empty(),
        "{}",
        declined.content()
    );
    let after = click(&mut sim, broke, &first, "Kabul et").await;
    assert!(
        after.ephemeral && after.content().contains("Bu meydan okuma sona erdi"),
        "{}",
        after.content()
    );

    // The host can take it back.
    let second = invite(&mut sim, a, b, "connect4", vec![int_option("bet", 100)]).await;
    let taken_back = click(&mut sim, a, &second, "Reddet").await;
    assert!(
        taken_back
            .content()
            .contains(&format!("{} meydan okumayı geri çekti", mention(a))),
        "{}",
        taken_back.content()
    );

    // Nobody can wager what they do not have.
    let too_much = invite(&mut sim, broke, a, "connect4", vec![int_option("bet", 100)]).await;
    assert!(
        too_much.ephemeral && too_much.content().contains("Yalnızca **30** kabuğun var"),
        "{}",
        too_much.content()
    );

    // Accepted: both 100 are held, and the winner takes the pot of 200.
    let third = invite(&mut sim, b, rich, "connect4", vec![int_option("bet", 100)]).await;
    let mut board = click(&mut sim, rich, &third, "Kabul et").await;
    assert_eq!((shells(&sim, b).await, shells(&sim, rich).await), (400, 400));
    assert!(
        board.description().contains("Havuz: **200** kabuk (kişi başı 100)"),
        "{}",
        board.description()
    );
    let (first, second) = (b, rich);
    for (player, label) in [
        (first, "1"),
        (second, "2"),
        (first, "1"),
        (second, "2"),
        (first, "1"),
        (second, "2"),
    ] {
        board = click(&mut sim, player, &board, label).await;
    }
    let won = click(&mut sim, first, &board, "1").await;
    assert!(
        won.content().contains("Kazanan **200** kabukluk havuzu alıyor"),
        "{}",
        won.content()
    );
    assert_eq!((shells(&sim, b).await, shells(&sim, rich).await), (600, 400));
}

#[tokio::test]
async fn duel_boards_stay_in_the_servers_language_whatever_the_clickers_is() {
    let Some(mut sim) = Sim::new("duel_boards_stay_in_the_servers_language_whatever_the_clickers_is").await else {
        return;
    };
    let (turk, english) = (unique(), unique());

    // A Turkish server: the English guest accepts a Turkish challenge and the board is still Turkish.
    turkish_server(&sim).await;
    sim.locale = "en-US";
    let challenge = invite(&mut sim, turk, english, "connect4", vec![]).await;
    assert!(challenge.content().contains("meydan okuyor"), "{}", challenge.content());
    let board = sim
        .component(
            english,
            &challenge.button("Kabul et").custom_id.clone().expect("id"),
            &[],
            &challenge,
        )
        .await;
    assert_eq!(board.title(), "Dört Bağla");
    assert!(
        board.content().contains(&format!("Sıra: {}", mention(turk))),
        "{}",
        board.content()
    );
    // What only the clicker sees is in the clicker's language.
    let refused = sim
        .component(english, &board.button("3").custom_id.clone().expect("id"), &[], &board)
        .await;
    assert!(
        refused.ephemeral && refused.content().contains("Not your turn yet"),
        "{}",
        refused.content()
    );
    // And a move by the English-speaking player redraws the board in Turkish all the same.
    let board = sim
        .component(turk, &board.button("3").custom_id.clone().expect("id"), &[], &board)
        .await;
    let board = sim
        .component(english, &board.button("4").custom_id.clone().expect("id"), &[], &board)
        .await;
    assert_eq!(board.title(), "Dört Bağla");
    assert!(
        board.content().contains(&format!("Sıra: {}", mention(turk))),
        "{}",
        board.content()
    );

    // An English server: the Turkish guest declines in Turkish, but the message everyone reads stays English.
    sim.app
        .set_guild_lang(sim.guild, Some(Lang::En))
        .await
        .expect("set the server's language");
    sim.locale = "tr";
    let challenge = invite(&mut sim, english, turk, "connect4", vec![]).await;
    assert!(challenge.content().contains("challenges"), "{}", challenge.content());
    let board = click(&mut sim, turk, &challenge, "Accept").await;
    assert_eq!(board.title(), "Connect four");
    let refused = click(&mut sim, turk, &board, "3").await;
    assert!(
        refused.ephemeral && refused.content().contains("Sıra henüz sende değil"),
        "{}",
        refused.content()
    );
}

#[tokio::test]
async fn duel_time_outs_are_in_the_servers_language() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) = Sim::new("duel_time_outs_are_in_the_servers_language").await else {
        return;
    };
    forget_late_games(&sim).await;
    turkish_server(&sim).await;
    sim.locale = "tr";
    let (a, b, c, d) = (unique(), unique(), unique(), unique());
    for user in [a, b, c, d] {
        fund(&sim, user, 300).await;
    }

    // An unanswered challenge expires on its message.
    let unanswered = invite(&mut sim, a, b, "rps", vec![int_option("bet", 50)]).await;
    let session = session_of(&unanswered);
    let message = unanswered.message_id;
    sim.app
        .db
        .set_game_message(session, CHANNEL_ID, message)
        .await
        .expect("message");
    expire(&sim, session).await;
    sweep_games(&sim.app).await.expect("sweep");
    let posts = sim.posts();
    let edit = posts
        .iter()
        .find(|p| p.message == message)
        .expect("the challenge message was edited");
    assert!(
        edit.content().contains("zamanında cevap vermedi") && edit.content().contains(&mention(b)),
        "{}",
        edit.content()
    );
    assert_eq!((shells(&sim, a).await, shells(&sim, b).await), (300, 300));

    // The player to move does nothing: the janitor ends the game, the other takes the pot, and the message
    // says so in the server's language.
    let challenge = invite(&mut sim, c, d, "connect4", vec![int_option("bet", 50)]).await;
    let board = click(&mut sim, d, &challenge, "Kabul et").await;
    let session = session_of(&board);
    expire(&sim, session).await;
    sweep_games(&sim.app).await.expect("sweep");
    assert_eq!((shells(&sim, c).await, shells(&sim, d).await), (250, 350));
    let message = message_of(&sim, session).await;
    let posts = sim.posts();
    let edit = posts
        .iter()
        .find(|p| p.message == message)
        .expect("the message says the game timed out");
    assert!(
        edit.content()
            .contains(&format!("{} için süre doldu, hükmen yenildi", mention(c)))
            && edit.content().contains("**100** kabukluk havuzu")
            && edit.content().contains(&mention(d)),
        "{}",
        edit.content()
    );
    assert_no_keys(edit.content());
    assert_eq!(edit.body["components"].as_array().map(Vec::len), Some(0));

    // A click on a late public game ends it in the server's language, even for an English clicker.
    let e = unique();
    fund(&sim, e, 300).await;
    let challenge = invite(&mut sim, e, d, "connect4", vec![int_option("bet", 50)]).await;
    let board = click(&mut sim, d, &challenge, "Kabul et").await;
    expire(&sim, session_of(&board)).await;
    sim.locale = "en-US";
    let ended = sim
        .component(e, &board.button("4").custom_id.clone().expect("id"), &[], &board)
        .await;
    assert!(
        ended.content().contains("için süre doldu, hükmen yenildi") && ended.components.is_empty(),
        "{}",
        ended.content()
    );
}

// /Rock paper scissors

#[tokio::test]
async fn rock_paper_scissors_in_turkish_with_and_without_a_bet() {
    let Some(mut sim) = Sim::new("rock_paper_scissors_in_turkish_with_and_without_a_bet").await else {
        return;
    };
    sim.locale = "tr";
    turkish_server(&sim).await;
    let (a, b, bystander) = (unique(), unique(), unique());
    fund(&sim, a, 300).await;
    fund(&sim, b, 300).await;

    let challenge = invite(&mut sim, a, b, "rps", vec![int_option("rounds", 3)]).await;
    assert!(
        challenge.content().contains("Taş kâğıt makas, 3 turluk maç"),
        "{}",
        challenge.content()
    );
    let board = click(&mut sim, b, &challenge, "Kabul et").await;
    assert!(board.files.is_empty(), "no picture, just the scores");
    assert!(
        board.content().contains("1. tur") && board.title() == "Taş kâğıt makas, 3 turluk maç",
        "{}",
        board.content()
    );
    assert!(
        board.content().contains("⏳") && board.content().contains("seçiyor"),
        "{}",
        board.content()
    );

    let refused = click(&mut sim, bystander, &board, "Hamleni seç").await;
    assert!(
        refused.ephemeral && refused.content().contains("Bu maçta değilsin"),
        "{}",
        refused.content()
    );

    // Each player gets a private picker.
    let a_picker = click(&mut sim, a, &board, "Hamleni seç").await;
    let b_picker = click(&mut sim, b, &board, "Hamleni seç").await;
    assert!(a_picker.ephemeral && b_picker.ephemeral);
    assert!(
        a_picker.content().contains("1. tur: hamleni seç"),
        "{}",
        a_picker.content()
    );
    for label in ["🪨 Taş", "📄 Kâğıt", "✂️ Makas"] {
        assert!(a_picker.has_button(label), "no {label}: {}", everything(&a_picker));
    }

    // A locks in rock: only A sees it, the public message just shows that A is ready.
    let locked = click(&mut sim, a, &a_picker, "🪨 Taş").await;
    assert!(
        locked.content().contains("🔒 🪨 Taş seçildi") && locked.components.is_empty(),
        "{}",
        locked.content()
    );
    let posts = sim.posts();
    let shown = posts.last().expect("the public message was refreshed");
    assert!(
        shown.content().contains("✅") && shown.content().contains("⏳") && shown.content().contains("seçti"),
        "{}",
        shown.content()
    );
    assert!(!shown.content().contains("Taş"));
    let again = click(&mut sim, a, &board, "Hamleni seç").await;
    assert!(
        again.ephemeral && again.content().contains("🪨 Taş seçtin"),
        "{}",
        again.content()
    );
    let repeat = click(&mut sim, a, &a_picker, "📄 Kâğıt").await;
    assert!(repeat.content().contains("🪨 Taş seçtin"), "{}", repeat.content());

    // B picks scissors: the round is revealed to the channel, rock breaks scissors.
    let revealed = click(&mut sim, b, &b_picker, "✂️ Makas").await;
    assert!(revealed.content().contains("Turu kaybettin."), "{}", revealed.content());
    let posts = sim.posts();
    let public = posts.last().expect("the reveal");
    assert!(public.content().contains("2. tur"), "{}", public.content());
    let description = public.embed()["description"].as_str().unwrap_or_default();
    assert!(
        description.contains("1 - 0") && description.contains("1. 🪨 vs ✂️") && description.contains("aldı"),
        "{description}"
    );
    let stale = click(&mut sim, b, &b_picker, "🪨 Taş").await;
    assert!(stale.content().contains("zaten bitti"), "{}", stale.content());

    // A tie is played again, then A wins the match.
    let board2 = reply_of(public);
    let a_picker = click(&mut sim, a, &board2, "Hamleni seç").await;
    let b_picker = click(&mut sim, b, &board2, "Hamleni seç").await;
    click(&mut sim, a, &a_picker, "📄 Kâğıt").await;
    let tie = click(&mut sim, b, &b_picker, "📄 Kâğıt").await;
    assert!(tie.content().contains("Berabere. Tekrar seç!"), "{}", tie.content());
    let posts = sim.posts();
    let public = posts.last().expect("the tie");
    assert!(
        public.content().contains("2. tur")
            && public.embed()["description"]
                .as_str()
                .unwrap_or_default()
                .contains("berabere, tekrar oynandı"),
        "{}",
        public.content()
    );
    let board3 = reply_of(public);
    let a_picker = click(&mut sim, a, &board3, "Hamleni seç").await;
    let b_picker = click(&mut sim, b, &board3, "Hamleni seç").await;
    click(&mut sim, b, &b_picker, "📄 Kâğıt").await;
    let won = click(&mut sim, a, &a_picker, "✂️ Makas").await;
    assert!(won.content().contains("Turu kazandın!"), "{}", won.content());
    let posts = sim.posts();
    let end = posts.last().expect("the final message");
    assert!(
        end.content().contains(&format!("{} maçı 2 - 0 kazandı", mention(a)))
            && end.content().contains("+**15** kabuk!"),
        "{}",
        end.content()
    );
    assert_no_keys(end.content());
    assert_eq!(end.body["components"].as_array().map(Vec::len), Some(0));
    assert_eq!((shells(&sim, a).await, shells(&sim, b).await), (315, 300));

    // With a wager the winner takes the pot.
    let (c, d) = (unique(), unique());
    fund(&sim, c, 300).await;
    fund(&sim, d, 300).await;
    let challenge = invite(
        &mut sim,
        c,
        d,
        "rps",
        vec![int_option("bet", 150), int_option("rounds", 1)],
    )
    .await;
    assert!(
        challenge.content().contains("Taş kâğıt makas, 1 turluk maç")
            && challenge.content().contains("kişi başı **150** kabuk"),
        "{}",
        challenge.content()
    );
    let board = click(&mut sim, d, &challenge, "Kabul et").await;
    assert_eq!((shells(&sim, c).await, shells(&sim, d).await), (150, 150));
    assert!(board.embeds[0].footer.is_some(), "the pot is in the footer");
    let footer = board.embeds[0]
        .footer
        .as_ref()
        .map(|f| f.text.clone())
        .unwrap_or_default();
    assert_eq!(footer, "Havuz: 300 kabuk");
    let c_picker = click(&mut sim, c, &board, "Hamleni seç").await;
    let d_picker = click(&mut sim, d, &board, "Hamleni seç").await;
    click(&mut sim, c, &c_picker, "🪨 Taş").await;
    click(&mut sim, d, &d_picker, "📄 Kâğıt").await;
    let posts = sim.posts();
    let end = posts.last().expect("the final message");
    assert!(
        end.content().contains(&format!("{} maçı 0 - 1 kazandı", mention(d)))
            && end.content().contains("Kazanan **300** kabukluk havuzu alıyor"),
        "{}",
        end.content()
    );
    assert_eq!((shells(&sim, c).await, shells(&sim, d).await), (150, 450));
}

// /Share or Grab

#[tokio::test]
async fn share_or_grab_in_turkish_hides_the_picks_and_pays_by_what_they_picked() {
    let Some(mut sim) = Sim::new("share_or_grab_in_turkish_hides_the_picks_and_pays_by_what_they_picked").await else {
        return;
    };
    sim.locale = "tr";
    turkish_server(&sim).await;
    let (a, b, bystander) = (unique(), unique(), unique());
    fund(&sim, a, 300).await;
    fund(&sim, b, 300).await;

    let challenge = invite(&mut sim, a, b, "share", vec![]).await;
    assert!(
        !challenge.ephemeral
            && challenge.content().contains("**Paylaş ya da Kap**")
            && challenge.content().contains("havuza **60** kabuk koyuyor"),
        "{}",
        challenge.content()
    );
    let board = click(&mut sim, b, &challenge, "Kabul et").await;
    assert_png(&board, 580, 350);
    assert!(
        board.content().contains("⏳") && board.has_button("Gizlice seç"),
        "{}",
        board.content()
    );
    assert!(
        board.content().contains("Havuzda **60** kabuk var"),
        "{}",
        board.content()
    );
    keep_png(&sim, &board, "share-tr-waiting");
    let refused = click(&mut sim, bystander, &board, "Gizlice seç").await;
    assert!(
        refused.ephemeral && refused.content().contains("Bu oyunda değilsin"),
        "{}",
        refused.content()
    );

    let a_picker = click(&mut sim, a, &board, "Gizlice seç").await;
    let b_picker = click(&mut sim, b, &board, "Gizlice seç").await;
    assert!(a_picker.ephemeral && b_picker.ephemeral);
    assert!(
        a_picker.content().contains("🤝 **Paylaş** havuzu böler. 🦀 **Kap**")
            && a_picker.content().contains("kimse bir şey alamaz"),
        "{}",
        a_picker.content()
    );
    let locked = click(&mut sim, a, &a_picker, "🤝 Paylaş").await;
    assert!(
        locked.content().contains("🔒 🤝 Paylaş seçildi") && locked.components.is_empty(),
        "{}",
        locked.content()
    );
    let posts = sim.posts();
    let shown = posts.last().expect("the public message was refreshed");
    assert!(
        shown.content().contains("✅") && shown.content().contains("⏳") && !shown.content().contains("Paylaş"),
        "{}",
        shown.content()
    );
    keep_post_png(&sim, shown, "share-tr-one-locked");
    let repeat = click(&mut sim, a, &a_picker, "🦀 Kap").await;
    assert!(repeat.content().contains("🤝 Paylaş seçtin"), "{}", repeat.content());

    let done = click(&mut sim, b, &b_picker, "🤝 Paylaş").await;
    assert!(
        done.content().contains("İkiniz de paylaştınız") && done.content().contains("Sen: 🤝 Paylaş, rakip: 🤝 Paylaş"),
        "{}",
        done.content()
    );
    let posts = sim.posts();
    let end = posts.last().expect("the result");
    assert!(
        end.content().contains("kişi başı **30** kabuk") && end.body["components"].as_array().map(Vec::len) == Some(0),
        "{}",
        end.content()
    );
    keep_post_png(&sim, end, "share-tr-both-shared");
    assert_eq!((shells(&sim, a).await, shells(&sim, b).await), (330, 330));
    let late = click(&mut sim, a, &a_picker, "🤝 Paylaş").await;
    assert!(late.content().contains("Bu oyun bitti"), "{}", late.content());

    // The same pair is not paid from the pool twice in an hour.
    let challenge = invite(&mut sim, b, a, "share", vec![]).await;
    let board = click(&mut sim, a, &challenge, "Kabul et").await;
    let (a_picker, b_picker) = (
        click(&mut sim, a, &board, "Gizlice seç").await,
        click(&mut sim, b, &board, "Gizlice seç").await,
    );
    click(&mut sim, a, &a_picker, "🤝 Paylaş").await;
    click(&mut sim, b, &b_picker, "🤝 Paylaş").await;
    let posts = sim.posts();
    let end = posts.last().expect("the result");
    assert!(
        end.content()
            .contains("son bir saat içinde denizin havuzundan ödül aldı"),
        "{}",
        end.content()
    );

    // Wagers: the grabber takes the pot while the other shared.
    let (a, b) = (unique(), unique());
    fund(&sim, a, 300).await;
    fund(&sim, b, 300).await;
    let challenge = invite(&mut sim, a, b, "share", vec![int_option("bet", 50)]).await;
    assert!(
        challenge
            .content()
            .contains("Yatırım: kişi başı **50** kabuk, havuzda **100** kabuk"),
        "{}",
        challenge.content()
    );
    let board = click(&mut sim, b, &challenge, "Kabul et").await;
    let (a_picker, b_picker) = (
        click(&mut sim, a, &board, "Gizlice seç").await,
        click(&mut sim, b, &board, "Gizlice seç").await,
    );
    assert!(
        a_picker.content().contains("**100** kabuk denize gömülür"),
        "{}",
        a_picker.content()
    );
    click(&mut sim, a, &a_picker, "🦀 Kap").await;
    let done = click(&mut sim, b, &b_picker, "🤝 Paylaş").await;
    assert!(done.content().contains("hepsini kaptı"), "{}", done.content());
    let posts = sim.posts();
    let end = posts.last().expect("the result");
    assert!(
        end.content().contains(&format!(
            "🦀 {}, {} paylaşırken **100** kabukluk havuzun tamamını kaptı!",
            mention(a),
            mention(b)
        )),
        "{}",
        end.content()
    );
    keep_post_png(&sim, end, "share-tr-grabbed");
    assert_eq!((shells(&sim, a).await, shells(&sim, b).await), (350, 250));
}

#[tokio::test]
async fn share_or_grab_forfeits_in_turkish_when_somebody_never_picks() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) = Sim::new("share_or_grab_forfeits_in_turkish_when_somebody_never_picks").await else {
        return;
    };
    forget_late_games(&sim).await;
    turkish_server(&sim).await;
    sim.locale = "tr";
    let (a, b, c, d) = (unique(), unique(), unique(), unique());
    for user in [a, b, c, d] {
        fund(&sim, user, 300).await;
    }
    // B never picks: the janitor ends the game, A takes the pot, in the server's language.
    let challenge = invite(&mut sim, a, b, "share", vec![int_option("bet", 50)]).await;
    let board = click(&mut sim, b, &challenge, "Kabul et").await;
    let session = session_of(&board);
    let a_picker = click(&mut sim, a, &board, "Gizlice seç").await;
    click(&mut sim, a, &a_picker, "🤝 Paylaş").await;
    // The public message was refreshed for A's pick; only what the janitor does next counts.
    sim.posts();
    expire(&sim, session).await;
    sweep_games(&sim.app).await.expect("sweep");
    assert_eq!((shells(&sim, a).await, shells(&sim, b).await), (350, 250));
    let message = message_of(&sim, session).await;
    let posts = sim.posts();
    let edit = posts
        .iter()
        .find(|p| p.message == message)
        .expect("the message says the game timed out");
    assert!(
        edit.content().contains(&format!(
            "⌛ {} için süre doldu, hükmen yenildi: **100** kabukluk havuzu {} alıyor.",
            mention(b),
            mention(a)
        )),
        "{}",
        edit.content()
    );
    keep_post_png(&sim, edit, "share-tr-forfeit");

    // Nobody picks: both stakes go back; and a click on the late game ends it in place.
    let challenge = invite(&mut sim, c, d, "share", vec![int_option("bet", 50)]).await;
    let board = click(&mut sim, d, &challenge, "Kabul et").await;
    expire(&sim, session_of(&board)).await;
    let ended = click(&mut sim, c, &board, "Gizlice seç").await;
    assert!(
        ended
            .content()
            .contains("Kimse zamanında seçmedi. İki yatırım da geri verildi."),
        "{}",
        ended.content()
    );
    assert_eq!((shells(&sim, c).await, shells(&sim, d).await), (300, 300));
}

// /Sea Fleet

/// Every custom id's game and counter belong to the buttons; the layout itself is in the database.
async fn fleets_of(sim: &Sim, session: i64) -> [Fleet; 2] {
    let (state,): (String,) = sqlx::query_as("SELECT state FROM game_sessions WHERE id = $1")
        .bind(session)
        .fetch_one(sim.app.db.pool())
        .await
        .expect("the session");
    let fields = state.split_once('|').map_or(state.as_str(), |(fields, _)| fields);
    let parts: Vec<&str> = fields.split(':').collect();
    assert_eq!(parts[0], "b", "the battle has begun: {fields}");
    [parts[2], parts[3]].map(|code| Fleet::decode(code).expect("a fleet"))
}

/// Fires at `cell` for `player`: a column, then a row.
async fn fire(sim: &mut Sim, player: u64, board: &Reply, cell: usize) -> Reply {
    let column = pick(sim, player, board, "bs:a:", &(cell % 10).to_string()).await;
    pick(sim, player, &column, "bs:f:", &(cell / 10).to_string()).await
}

#[tokio::test]
async fn sea_fleet_in_turkish_places_fires_sinks_a_ship_and_resigns() {
    let Some(mut sim) = Sim::new("sea_fleet_in_turkish_places_fires_sinks_a_ship_and_resigns").await else {
        return;
    };
    sim.locale = "tr";
    turkish_server(&sim).await;
    let (a, b, stranger) = (unique(), unique(), unique());
    fund(&sim, a, 300).await;
    fund(&sim, b, 300).await;

    let challenge = invite(&mut sim, a, b, "seafleet", vec![int_option("bet", 100)]).await;
    assert!(
        challenge.content().contains("**Deniz Filosu**") && challenge.content().contains("hükmen yenilir"),
        "{}",
        challenge.content()
    );
    let public = click(&mut sim, b, &challenge, "Kabul et").await;
    assert_eq!(public.files[0].0, "seafleet.png");
    assert_eq!(public.title(), "Deniz Filosu");
    assert!(
        public.content().contains("filosunu gizlice yerleştirmeli"),
        "{}",
        public.content()
    );
    assert!(
        public
            .description()
            .contains("Her kaptan beş gemisini gizlice yerleştirir")
            && public.description().contains("Havuz: **200** kabuk (kişi başı 100)"),
        "{}",
        public.description()
    );
    keep_png(&sim, &public, "seafleet-tr-public-placing");
    let session = session_of(&public);

    // Each captain places in private.
    let outsider = click(&mut sim, stranger, &public, "Filomu yerleştir").await;
    assert!(
        outsider.ephemeral && outsider.content().contains("Bu oyunda değilsin"),
        "{}",
        outsider.content()
    );
    let open = click(&mut sim, a, &public, "Filomu yerleştir").await;
    assert!(open.ephemeral, "the placement is private");
    assert_eq!(open.title(), "Filonu yerleştir");
    assert!(
        open.description().contains("Bunu yalnızca sen görüyorsun"),
        "{}",
        open.description()
    );
    for label in ["Karıştır", "Filoyu kilitle"] {
        assert!(open.has_button(label), "no {label}: {}", everything(&open));
    }
    keep_png(&sim, &open, "seafleet-tr-place");
    let shuffled = click(&mut sim, a, &open, "Karıştır").await;
    assert!(shuffled.ephemeral && shuffled.has_button("Filoyu kilitle"));
    let early = sim.component(a, &format!("bs:a:{session}:0"), &["3"], &public).await;
    assert!(
        early.ephemeral && early.content().contains("Savaş henüz başlamadı"),
        "{}",
        early.content()
    );
    let locked = click(&mut sim, a, &shuffled, "Filoyu kilitle").await;
    assert!(
        locked.components.is_empty() && locked.title() == "Filon",
        "{}",
        locked.title()
    );
    keep_png(&sim, &locked, "seafleet-tr-locked");
    let again = click(&mut sim, a, &public, "Filomu yerleştir").await;
    assert!(
        again.ephemeral && again.content().contains("Filon kilitlendi"),
        "{}",
        again.content()
    );
    let waiting = latest_post(&sim);
    assert!(
        waiting.content().contains("filosunu gizlice yerleştirmeli"),
        "{}",
        waiting.content()
    );

    // The second captain locks in too, which starts the battle on the public message.
    let open = click(&mut sim, b, &public, "Filomu yerleştir").await;
    click(&mut sim, b, &open, "Filoyu kilitle").await;
    let post = latest_post(&sim);
    let battle = reply_of(&post);
    assert!(
        battle.content().contains("Filolar denizde!") && battle.content().contains("ateş etmeli"),
        "{}",
        battle.content()
    );
    assert_no_keys(&everything(&battle));
    assert!(battle.has_button("Filom") && battle.has_button("Pes et"));
    assert_eq!(battle.options("bs:a:").len(), 10, "ten columns to pick from");
    assert!(
        battle.options("bs:a:")[0].label == "A sütunu",
        "{}",
        battle.options("bs:a:")[0].label
    );
    keep_post_png(&sim, &post, "seafleet-tr-battle");
    let fleets = fleets_of(&sim, session).await;
    let first = mover(&battle, a, b);
    let second = if first == a { b } else { a };
    let (mine, theirs) = {
        let first_index = usize::from(first != a);
        (&fleets[first_index], &fleets[1 - first_index])
    };
    let water = (0..fleet::CELLS)
        .find(|&c| theirs.grid()[c].is_none())
        .expect("some water");
    let ship = mine.cells_of(0)[0];

    // Not your turn; a miss; a hit; the turn passes either way.
    let early = sim
        .component(second, &battle.menu("bs:a:").custom_id.clone(), &["3"], &battle)
        .await;
    assert!(
        early.ephemeral && early.content().contains("Sıra henüz sende değil"),
        "{}",
        early.content()
    );
    let column = pick(&mut sim, first, &battle, "bs:a:", &(water % 10).to_string()).await;
    assert!(
        column.content().contains(&format!(
            "**{}** sütunu seçildi: şimdi satırı seç.",
            char::from(b'A' + (water % 10) as u8)
        )),
        "{}",
        column.content()
    );
    assert!(
        column
            .menu("bs:f:")
            .placeholder
            .as_deref()
            .unwrap_or_default()
            .contains("vurulacak satırı seç"),
        "{}",
        everything(&column)
    );
    let miss = pick(&mut sim, first, &column, "bs:f:", &(water / 10).to_string()).await;
    assert!(
        miss.content().contains(&format!("{}: ıska.", fleet::cell_name(water))),
        "{}",
        miss.content()
    );
    assert_eq!(mover(&miss, a, b), second);
    let hit = fire(&mut sim, second, &miss, ship).await;
    assert!(
        hit.content().contains(&format!("{}: isabet!", fleet::cell_name(ship))),
        "{}",
        hit.content()
    );
    keep_png(&sim, &hit, "seafleet-tr-after-a-hit");

    // A captain looks at their own waters in private.
    let own = click(&mut sim, second, &hit, "Filom").await;
    assert!(own.ephemeral && own.has_button("Yenile"), "{}", own.content());
    assert!(
        own.description().contains("Diğer kaptan ateş ettikten sonra"),
        "{}",
        own.description()
    );
    keep_png(&sim, &own, "seafleet-tr-own-waters");

    // Sinking the dinghy (two cells) of the other fleet is named.
    let mut board = hit;
    let dinghy = theirs.cells_of(4);
    let (shooter, target) = (first, dinghy.clone());
    // The first captain is on turn again after the second fired; the second then fires into the first's sea
    // at a free cell, and so on, until the dinghy is down.
    let free: Vec<usize> = (0..fleet::CELLS).filter(|&c| mine.grid()[c].is_none()).collect();
    let mut sunk_line = None;
    for (n, cell) in target.iter().enumerate() {
        board = fire(&mut sim, shooter, &board, *cell).await;
        if n + 1 < target.len() {
            board = fire(&mut sim, second, &board, free[n]).await;
        } else {
            sunk_line = Some(board.content().to_owned());
        }
    }
    let sunk_line = sunk_line.expect("the last shot");
    assert!(sunk_line.contains("**Sandal battı!**"), "{sunk_line}");
    keep_png(&sim, &board, "seafleet-tr-dinghy-sunk");

    // Resigning asks first; the other captain wins the pot and the public radar says so.
    let ask = click(&mut sim, second, &board, "Pes et").await;
    assert!(
        ask.ephemeral && ask.has_button("Evet, pes et") && ask.content().contains("Pes etmek istiyor musun"),
        "{}",
        ask.content()
    );
    let done = click(&mut sim, second, &ask, "Evet, pes et").await;
    assert!(done.content().contains("Pes ettin."), "{}", done.content());
    let post = latest_post(&sim);
    assert!(
        post.content().contains(&mention(first)) && post.content().contains("pes etti!"),
        "{}",
        post.content()
    );
    assert!(
        post.content().contains("**200** kabukluk havuzu alıyor"),
        "{}",
        post.content()
    );
    assert_eq!(post.body["components"].as_array().map(Vec::len), Some(0));
    keep_post_png(&sim, &post, "seafleet-tr-resigned");
    let again = click(&mut sim, second, &ask, "Evet, pes et").await;
    assert!(
        again.ephemeral && again.content().contains("Bu oyun bitti"),
        "{}",
        again.content()
    );
}

// /Pearl Checkers

#[tokio::test]
async fn pearl_checkers_in_turkish_moves_refuses_and_resigns() {
    let Some(mut sim) = Sim::new("pearl_checkers_in_turkish_moves_refuses_and_resigns").await else {
        return;
    };
    sim.locale = "tr";
    turkish_server(&sim).await;
    let (a, b, stranger) = (unique(), unique(), unique());
    fund(&sim, a, 300).await;
    fund(&sim, b, 300).await;

    let challenge = invite(&mut sim, a, b, "checkers", vec![int_option("bet", 100)]).await;
    assert!(
        challenge.content().contains("**İnci Dama**") && challenge.content().contains("Bahis: kişi başı **100** kabuk"),
        "{}",
        challenge.content()
    );
    let board = click(&mut sim, b, &challenge, "Kabul et").await;
    assert_eq!(board.files[0].0, "checkers.png");
    assert_eq!(board.title(), "İnci Dama");
    assert!(
        board.description().contains("İlk hamle incilerde")
            && board.description().contains("Havuz: **200** kabuk (kişi başı 100)"),
        "{}",
        board.description()
    );
    assert!(
        board.content().contains("(inciler)") && board.content().contains("(siyah inciler)"),
        "{}",
        board.content()
    );
    keep_png(&sim, &board, "checkers-tr-start");
    let first = mover(&board, a, b);
    let second = if first == a { b } else { a };

    // The wrong player, a stranger and a piece with no moves are all turned away.
    let wrong = sim
        .component(second, &board.menu("ck:s:").custom_id.clone(), &["9"], &board)
        .await;
    assert!(
        wrong.ephemeral && wrong.content().contains("Sıra henüz sende değil"),
        "{}",
        wrong.content()
    );
    let outsider = sim
        .component(stranger, &board.menu("ck:s:").custom_id.clone(), &["9"], &board)
        .await;
    assert!(
        outsider.ephemeral && outsider.content().contains("Bu oyunda değilsin"),
        "{}",
        outsider.content()
    );
    let nothing = sim
        .component(first, &board.menu("ck:s:").custom_id.clone(), &["0"], &board)
        .await;
    assert!(
        nothing.ephemeral && nothing.content().contains("Bu taş hareket edemiyor"),
        "{}",
        nothing.content()
    );

    // The piece menu names the piece and how many moves it has; picking one shows where it can go.
    let pieces = board.options("ck:s:").to_vec();
    assert!(
        pieces
            .iter()
            .all(|o| o.label.starts_with("Taş ") && o.description.as_deref().is_some_and(|d| d.ends_with("hamle"))),
        "{pieces:?}"
    );
    assert_eq!(board.menu("ck:s:").placeholder.as_deref(), Some("Oynatacağın taşı seç"));
    let value = pieces[0].value.clone();
    let picked = pick(&mut sim, first, &board, "ck:s:", &value).await;
    assert!(
        picked
            .menu("ck:t:")
            .placeholder
            .as_deref()
            .unwrap_or_default()
            .contains("karesindeki taş nereye gitsin?"),
        "{}",
        everything(&picked)
    );
    keep_png(&sim, &picked, "checkers-tr-piece-picked");
    let target = picked.options("ck:t:")[0].value.clone();
    let moved = pick(&mut sim, first, &picked, "ck:t:", &target).await;
    assert_eq!(mover(&moved, a, b), second, "the turn passes");
    assert!(
        moved.description().contains("1. hamle") || moved.description().contains("hamle"),
        "{}",
        moved.description()
    );
    keep_png(&sim, &moved, "checkers-tr-after-a-move");

    // Resigning asks first; the other player wins the pot and the public board says so.
    let ask = click(&mut sim, second, &moved, "Pes et").await;
    assert!(ask.ephemeral && ask.has_button("Evet, pes et"), "{}", ask.content());
    assert_eq!(
        shells(&sim, a).await + shells(&sim, b).await,
        400,
        "asking does not end the game"
    );
    let done = click(&mut sim, second, &ask, "Evet, pes et").await;
    assert!(done.content().contains("Pes ettin."), "{}", done.content());
    let post = latest_post(&sim);
    assert!(
        post.content().contains(&mention(first)) && post.content().contains("pes etti"),
        "{}",
        post.content()
    );
    assert!(
        post.content().contains("**200** kabukluk havuzu alıyor"),
        "{}",
        post.content()
    );
    assert_eq!(post.body["components"].as_array().map(Vec::len), Some(0));
    keep_post_png(&sim, &post, "checkers-tr-resigned");
    assert_eq!(shells(&sim, first).await, 400, "the winner took both wagers");
    let late = click(&mut sim, second, &ask, "Evet, pes et").await;
    assert!(
        late.ephemeral && late.content().contains("Bu oyun bitti"),
        "{}",
        late.content()
    );
}

// /Reef Chess

/// Makes one move of chess (`e2e4`) for whoever is to move.
async fn chess_move(sim: &mut Sim, board: &Reply, a: u64, b: u64, uci: &str) -> Reply {
    let player = mover(board, a, b);
    let picked = pick(sim, player, board, "cc:s:", &uci[..2]).await;
    pick(sim, player, &picked, "cc:t:", uci).await
}

#[tokio::test]
async fn reef_chess_in_turkish_plays_to_a_mate_and_names_its_moves() {
    let Some(mut sim) = Sim::new("reef_chess_in_turkish_plays_to_a_mate_and_names_its_moves").await else {
        return;
    };
    sim.locale = "tr";
    turkish_server(&sim).await;
    let (a, b, stranger) = (unique(), unique(), unique());

    let challenge = invite(&mut sim, a, b, "chess", vec![]).await;
    assert!(
        challenge.content().contains("**Resif Satrancı**") && challenge.content().contains("Bahis yok"),
        "{}",
        challenge.content()
    );
    let board = click(&mut sim, b, &challenge, "Kabul et").await;
    assert_eq!(board.files[0].0, "chess.png");
    assert_eq!(board.title(), "Resif Satrancı");
    assert!(
        board.description().contains("İlk hamle mercanda"),
        "{}",
        board.description()
    );
    assert!(
        board.content().contains("(mercan)") && board.content().contains("(gök)"),
        "{}",
        board.content()
    );
    keep_png(&sim, &board, "chess-tr-start");
    let pieces = board.options("cc:s:").to_vec();
    let labels: Vec<_> = pieces
        .iter()
        .map(|o| (o.label.as_str(), o.description.as_deref().unwrap_or_default()))
        .collect();
    assert!(
        labels.contains(&("Piyon e2", "İnci, 2 hamle")) && labels.contains(&("At g1", "Denizatı, 2 hamle")),
        "{labels:?}"
    );
    let first = mover(&board, a, b);
    let second = if first == a { b } else { a };

    // The wrong player, a stranger and a piece with no moves are turned away.
    let wrong = sim
        .component(second, &board.menu("cc:s:").custom_id.clone(), &["e7"], &board)
        .await;
    assert!(
        wrong.ephemeral && wrong.content().contains("Sıra henüz sende değil"),
        "{}",
        wrong.content()
    );
    let outsider = sim
        .component(stranger, &board.menu("cc:s:").custom_id.clone(), &["e2"], &board)
        .await;
    assert!(
        outsider.ephemeral && outsider.content().contains("Bu oyunda değilsin"),
        "{}",
        outsider.content()
    );
    let nothing = sim
        .component(first, &board.menu("cc:s:").custom_id.clone(), &["a1"], &board)
        .await;
    assert!(
        nothing.ephemeral && nothing.content().contains("Bu taş hareket edemiyor"),
        "{}",
        nothing.content()
    );
    let picked = pick(&mut sim, first, &board, "cc:s:", "e2").await;
    assert!(
        picked
            .menu("cc:t:")
            .placeholder
            .as_deref()
            .unwrap_or_default()
            .contains("e2 karesindeki piyon nereye gitsin?"),
        "{}",
        everything(&picked)
    );
    keep_png(&sim, &picked, "chess-tr-piece-picked");
    let illegal = sim
        .component(first, &picked.menu("cc:t:").custom_id.clone(), &["e2e5"], &picked)
        .await;
    assert!(
        illegal.ephemeral && illegal.content().contains("Bu hamle geçerli değil"),
        "{}",
        illegal.content()
    );

    // Fool's mate: the game ends in checkmate, and a game this short earns no shells.
    let mut board = chess_move(&mut sim, &board, a, b, "f2f3").await;
    for uci in ["e7e5", "g2g4"] {
        board = chess_move(&mut sim, &board, a, b, uci).await;
    }
    keep_png(&sim, &board, "chess-tr-before-the-mate");
    let mated = chess_move(&mut sim, &board, a, b, "d8h4").await;
    assert!(
        mated.content().contains("kazandı: şah mat!")
            && mated.content().contains("(Bu kadar kısa bir oyun kabuk kazandırmaz.)"),
        "{}",
        mated.content()
    );
    assert!(mated.components.is_empty());
    keep_png(&sim, &mated, "chess-tr-checkmate");

    // A new game: check, castling, promotion and resigning. The position is set under the board.
    let (c, d) = (unique(), unique());
    fund(&sim, c, 300).await;
    fund(&sim, d, 300).await;
    let challenge = invite(&mut sim, c, d, "chess", vec![int_option("bet", 50)]).await;
    let board = click(&mut sim, d, &challenge, "Kabul et").await;
    let session = session_of(&board);
    set_fields(&sim, session, "0:e2e4 f7f6 d1h5").await;
    let current = sim
        .component(c, &board.menu("cc:s:").custom_id.clone(), &["e1"], &board)
        .await;
    assert!(
        current.content().contains(&format!("Sıra: {}", mention(d))),
        "black is to move, in check: {}",
        current.content()
    );
    keep_png(&sim, &current, "chess-tr-in-check");
    set_fields(
        &sim,
        session,
        "0:e2e4 e7e5 g1f3 b8c6 f1c4 g8f6 d2d3 d7d6 c1g5 c8g4 b1c3 a7a6 d1d2 h7h6",
    )
    .await;
    let current = sim
        .component(c, &current.menu("cc:s:").custom_id.clone(), &["e1"], &current)
        .await;
    let picked = pick(&mut sim, c, &current, "cc:s:", "e1").await;
    let labels: Vec<_> = picked.options("cc:t:").iter().map(|o| o.label.clone()).collect();
    assert!(labels.iter().any(|l| l.contains("(kısa rok)")), "{labels:?}");
    assert!(labels.iter().any(|l| l.contains("(uzun rok)")), "{labels:?}");
    let castled = pick(&mut sim, c, &picked, "cc:t:", "e1g1").await;
    assert!(castled.description().contains("O-O"), "{}", castled.description());
    set_fields(&sim, session, "0:a2a4 b7b5 a4b5 a7a6 b5a6 c8b7 a6a7 g8f6").await;
    let current = sim
        .component(c, &castled.menu("cc:s:").custom_id.clone(), &["a7"], &castled)
        .await;
    let picked = pick(&mut sim, c, &current, "cc:s:", "a7").await;
    assert!(
        picked.options("cc:t:")[0].label.contains("(vezir olur)"),
        "{}",
        picked.options("cc:t:")[0].label
    );
    assert!(
        picked.options("cc:t:").iter().any(|o| o.label.contains("(at olur)")),
        "{:?}",
        picked.options("cc:t:")
    );
    let promoted = pick(&mut sim, c, &picked, "cc:t:", "a7b8n").await;
    assert!(promoted.description().contains("axb8=N"), "{}", promoted.description());

    // Resigning asks first; the other player wins the pot and the public board says so.
    let ask = click(&mut sim, d, &promoted, "Pes et").await;
    assert!(ask.ephemeral && ask.has_button("Evet, pes et"), "{}", ask.content());
    assert_eq!(shells(&sim, c).await, 250, "asking does not end the game");
    let done = click(&mut sim, d, &ask, "Evet, pes et").await;
    assert!(done.content().contains("Pes ettin."), "{}", done.content());
    assert_eq!((shells(&sim, c).await, shells(&sim, d).await), (350, 250));
    let post = latest_post(&sim);
    assert!(
        post.content().contains("kazandı: ") && post.content().contains("pes etti!"),
        "{}",
        post.content()
    );
    assert!(post.content().contains(&mention(c)), "{}", post.content());
    keep_post_png(&sim, &post, "chess-tr-resigned");
}
