//! The feature hooks (`hooks.rs`) are empty until the phase-4 packages fill them: what reaches them must
//! pass through quietly. A direct message and a reaction have no handler of their own yet.

use pinchora_commands::{ReactionEvent, on_reaction};
use twilight_model::gateway::payload::incoming::ReactionAdd;

use super::*;

#[tokio::test]
async fn direct_messages_and_reactions_pass_the_empty_hooks_quietly() {
    let Some(sim) = Sim::new("direct_messages_and_reactions_pass_the_empty_hooks_quietly").await else {
        return;
    };
    let user = unique();
    // A message with no server: Discord leaves out `guild_id` and `member`.
    let dm: MessageCreate = serde_json::from_value(json!({
        "id": unique().to_string(),
        "channel_id": unique().to_string(),
        "type": 0,
        "author": user_json(user),
        "content": "hello bot",
        "timestamp": "2024-01-01T00:00:00.000000+00:00",
        "edited_timestamp": null,
        "tts": false,
        "mention_everyone": false,
        "mention_roles": [],
        "mentions": [],
        "pinned": false,
        "attachments": [],
        "embeds": [],
        "flags": 0,
    }))
    .expect("direct message JSON");
    on_message(sim.app.clone(), dm)
        .await
        .expect("the message handler failed");

    let reaction: ReactionAdd = serde_json::from_value(json!({
        "burst": false,
        "channel_id": CHANNEL_ID.to_string(),
        "emoji": { "id": null, "name": "⭐" },
        "guild_id": sim.guild.to_string(),
        "message_id": unique().to_string(),
        "user_id": user.to_string(),
    }))
    .expect("reaction JSON");
    on_reaction(sim.app.clone(), ReactionEvent::Add(Box::new(reaction)))
        .await
        .expect("the reaction handler failed");

    // Chat in a server still earns its XP minute and says nothing.
    sim.say(user, &[]).await;
    assert!(
        sim.posts().is_empty(),
        "the hooks said something: {:?}",
        sim.posts().len()
    );
}
