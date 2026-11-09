use crate::{Sim, json, unique};
use tracing::instrument::WithSubscriber;

#[tokio::test]
async fn discord_interaction_has_one_span_with_request_id() {
    let mut sim = Sim::new("interaction_span").await.expect("test services");
    let spans = pinchora_testkit::traces::Spans::default();
    let reply = sim
        .command(unique(), "ping", json!([]))
        .with_subscriber(spans.dispatch())
        .await;
    assert_eq!(reply.title(), "Pong!");
    let recorded = spans.named("discord.interaction");
    assert_eq!(recorded.len(), 1);
    assert!(recorded[0]["request_id"].parse::<u64>().unwrap() > 0);
}

#[tokio::test]
async fn gateway_interaction_has_one_span_with_request_id() {
    let sim = Sim::new("gateway_interaction_span").await.expect("test services");
    let interaction = sim.interaction(
        unique(),
        2,
        json!({"id": unique().to_string(), "name": "ping", "type": 1, "options": []}),
        None,
    );
    let request_id = interaction.id.to_string();
    let spans = pinchora_testkit::traces::Spans::default();
    let (tx, mut rx) = tokio::sync::oneshot::channel();
    let responder = std::sync::Arc::new(crate::Responder::http(sim.app.clone(), &interaction, tx));
    pinchora_commands::handle_gateway_interaction(sim.app.clone(), interaction, responder)
        .with_subscriber(spans.dispatch())
        .await;
    assert!(rx.try_recv().is_ok());
    let recorded = spans.named("discord.interaction");
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0]["request_id"], request_id);
}
