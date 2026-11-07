//! Welcome and goodbye images from the dashboard's editor: every way a saved design is sent, goodbye images,
//! direct messages (open and closed), a stored design that no longer reads, and `/setup welcome card`
//! putting the classic card back. Pictures land in `target/sim/<test>/`.

use pinchora_core::card_design::{CardKind, Delivery};
use pinchora_render::templates;

use super::*;

/// The size of the one picture a post carries.
fn picture(post: &Post) -> (u32, u32) {
    let [(name, bytes)] = &post.files[..] else {
        panic!("expected one picture, got {}", post.files.len());
    };
    let image = image::load_from_memory(bytes).unwrap_or_else(|e| panic!("{name} is not a png: {e}"));
    (image.width(), image.height())
}

/// Saves `key`'s template as the server's `kind` design, sent as `delivery`.
async fn save(sim: &Sim, kind: CardKind, key: &str, delivery: Delivery) {
    let design = templates::template(key, kind).expect("a template");
    let json = serde_json::to_string(&design).unwrap();
    sim.app
        .db
        .put_card_design(sim.guild, kind, Some(&json), delivery)
        .await
        .unwrap();
}

#[tokio::test]
async fn joins_and_leaves_send_the_saved_designs_the_chosen_way() {
    let Some(sim) = Sim::new("joins_and_leaves_send_the_saved_designs_the_chosen_way").await else {
        return;
    };
    let (welcome, bye) = (unique(), unique());
    let db = &sim.app.db;
    db.update_welcome(sim.guild, Some(true), Some(welcome), Some("Hi {user}!"))
        .await
        .unwrap();
    db.update_goodbye(sim.guild, Some(bye), Some("Bye {user}"))
        .await
        .unwrap();
    sim.app.forget_guild_settings(sim.guild).await;

    // Never opened the editor: the classic card under the text, in one message.
    let ana = unique();
    sim.join(ana, "Ana", false, None).await;
    let posts = sim.posts();
    let [classic] = &in_channel(&posts, welcome)[..] else {
        panic!("expected one welcome, got {}", posts.len());
    };
    assert_eq!(classic.content(), format!("Hi <@{ana}>!"));
    assert_eq!(picture(classic), (1100, 500));
    assert_eq!(classic.files[0].0, "welcome.png");

    // The image first, then the text on its own.
    save(&sim, CardKind::Welcome, "starlit-banner", Delivery::BeforeMessage).await;
    let ben = unique();
    sim.join(ben, "Ben", false, None).await;
    let posts = sim.posts();
    let [image, text] = &in_channel(&posts, welcome)[..] else {
        panic!("expected two messages, got {}", posts.len());
    };
    assert_eq!((picture(image), image.content()), ((1200, 400), ""));
    assert_eq!(text.content(), format!("Hi <@{ben}>!"));
    assert!(text.files.is_empty());
    assert_eq!(text.body["allowed_mentions"]["users"], json!([ben.to_string()]));

    // Only the image, then only the text.
    save(&sim, CardKind::Welcome, "pet-party", Delivery::ImageOnly).await;
    sim.join(unique(), "Cy", false, None).await;
    let posts = sim.posts();
    let [alone] = &posts[..] else {
        panic!("expected one message, got {}", posts.len());
    };
    assert_eq!((picture(alone), alone.content()), ((1100, 500), ""));
    save(&sim, CardKind::Welcome, "pet-party", Delivery::TextOnly).await;
    let di = unique();
    sim.join(di, "Di", false, None).await;
    let posts = sim.posts();
    let [words] = &posts[..] else {
        panic!("expected one message, got {}", posts.len());
    };
    assert_eq!(words.content(), format!("Hi <@{di}>!"));
    assert!(words.files.is_empty());

    // By direct message: nothing in the channel. Closed DMs are skipped quietly.
    save(&sim, CardKind::Welcome, "clean-card", Delivery::Dm).await;
    let ed = unique();
    sim.join(ed, "Ed", false, None).await;
    let posts = sim.posts();
    let [dm] = &posts[..] else {
        panic!("expected one direct message, got {}", posts.len());
    };
    assert_eq!(dm.channel, ed, "the mock's DM channel is the member's id");
    assert_eq!(
        (picture(dm), dm.content()),
        ((1000, 360), format!("Hi <@{ed}>!").as_str())
    );
    let shy = unique();
    DM_OVERRIDES.lock().unwrap().insert(shy, true);
    sim.join(shy, "Shy", false, None).await;
    assert!(sim.posts().is_empty());

    // A goodbye image goes with the goodbye text.
    save(&sim, CardKind::Goodbye, "clean-card", Delivery::WithMessage).await;
    sim.leave(ana, "Ana", false).await;
    let posts = sim.posts();
    let [farewell] = &in_channel(&posts, bye)[..] else {
        panic!("expected one goodbye, got {}", posts.len());
    };
    assert_eq!(farewell.content(), "Bye Ana");
    assert_eq!(picture(farewell), (1000, 360));
    assert_eq!(farewell.files[0].0, "goodbye.png");
    assert_eq!(farewell.body["allowed_mentions"]["parse"], json!([]));

    // A stored design that no longer reads falls back to the classic card, sent the way chosen.
    sim.app
        .db
        .put_card_design(
            sim.guild,
            CardKind::Welcome,
            Some(r#"{"version": 1}"#),
            Delivery::ImageOnly,
        )
        .await
        .unwrap();
    sim.join(unique(), "Flo", false, None).await;
    let posts = sim.posts();
    let [fallback] = &posts[..] else {
        panic!("expected one message, got {}", posts.len());
    };
    assert_eq!((picture(fallback), fallback.content()), ((1100, 500), ""));
}

#[tokio::test]
async fn config_welcome_card_puts_the_classic_card_back() {
    let Some(mut sim) = Sim::new("config_welcome_card_puts_the_classic_card_back").await else {
        return;
    };
    let manager = unique();
    sim.permissions = MANAGE_GUILD;
    save(&sim, CardKind::Welcome, "starlit-banner", Delivery::BeforeMessage).await;

    // Without options it shows the editor's design as it is.
    let shown = sim
        .command(manager, "config", json!([sub("welcome-card", vec![])]))
        .await;
    assert_eq!(size_of_reply(&shown), (1200, 400));
    assert!(
        shown.description().contains("design made in the dashboard's editor"),
        "{}",
        shown.description()
    );

    // Any option goes back to the classic card in that theme, and says so; the delivery stays.
    let classic = sim
        .command(
            manager,
            "config",
            json!([sub("welcome-card", vec![text_option("theme", "forest")])]),
        )
        .await;
    assert_eq!(size_of_reply(&classic), (1100, 500));
    assert!(
        classic.description().contains("replaced the design"),
        "{}",
        classic.description()
    );
    let row = sim
        .app
        .db
        .card_design(sim.guild, CardKind::Welcome)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (row.design.as_deref(), row.delivery(CardKind::Welcome)),
        (None, Delivery::BeforeMessage)
    );
    let again = sim
        .command(
            manager,
            "config",
            json!([sub("welcome-card", vec![text_option("title", "HOWDY")])]),
        )
        .await;
    assert!(!again.description().contains("replaced"), "nothing left to replace");
    sim.permissions = 0;
}

/// A reply's picture size.
fn size_of_reply(reply: &Reply) -> (u32, u32) {
    let (name, bytes) = reply.files.first().expect("a picture");
    let image = image::load_from_memory(bytes).unwrap_or_else(|e| panic!("{name} is not a png: {e}"));
    (image.width(), image.height())
}
