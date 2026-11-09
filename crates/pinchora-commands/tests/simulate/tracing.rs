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
