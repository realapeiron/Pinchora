//! Community v2 personal tools through registered commands in both supported languages.
use super::*;

#[tokio::test]
async fn checklist_and_highlights_are_private_and_bilingual() {
    let Some(mut sim) = Sim::new("checklist_and_highlights_are_private_and_bilingual").await else {
        return;
    };
    let member = unique();
    let other = unique();
    let added = sim
        .command(
            member,
            "checklist",
            json!([sub("add", vec![text_option("text", "Plan the event")])]),
        )
        .await;
    assert!(added.ephemeral && added.content().contains("Added"), "{added}");
    let listed = sim.command(member, "checklist", json!([sub("list", vec![])])).await;
    assert!(
        listed.ephemeral && listed.content().contains("Plan the event"),
        "{listed}"
    );
    let someone_else = sim.command(other, "checklist", json!([sub("list", vec![])])).await;
    assert!(!someone_else.content().contains("Plan the event"), "{someone_else}");

    sim.locale = "tr";
    let followed = sim
        .command(
            member,
            "highlight",
            json!([sub("add", vec![text_option("word", "mercan")])]),
        )
        .await;
    assert!(
        followed.ephemeral && followed.content().contains("mercan"),
        "{followed}"
    );
    let words = sim.command(member, "highlight", json!([sub("list", vec![])])).await;
    assert!(words.ephemeral && words.content().contains("mercan"), "{words}");
    let other_words = sim.command(other, "highlight", json!([sub("list", vec![])])).await;
    assert!(!other_words.content().contains("mercan"), "{other_words}");
}

#[tokio::test]
async fn highlights_refuse_without_message_content_intent() {
    let Some(mut sim) = Sim::with("highlights_refuse_without_message_content_intent", |app| {
        app.message_content = false;
    })
    .await
    else {
        return;
    };
    let reply = sim
        .command(
            unique(),
            "highlight",
            json!([sub("add", vec![text_option("word", "coral")])]),
        )
        .await;
    assert!(
        reply.ephemeral && reply.content().contains("Message Content"),
        "{reply}"
    );
}
