mod app;
mod commands;
mod components;
mod cx;
mod events;
mod game;
mod responder;
mod util;

pub use app::App;
pub use commands::definitions as command_definitions;
pub use cx::Cx;
pub use events::{on_member_add, on_message};
pub use responder::Responder;

use std::{sync::Arc, time::Instant};

use twilight_model::application::interaction::{Interaction, InteractionData};

/// Runs one interaction to completion. Every path answers the user, errors included.
pub async fn handle_interaction(app: Arc<App>, mut interaction: Interaction, responder: Arc<Responder>) {
    let started = Instant::now();
    let data = interaction.data.take();
    let cx = Cx {
        app,
        interaction,
        responder,
    };
    let (label, result) = match data {
        Some(InteractionData::ApplicationCommand(command)) => {
            let label = command.name.clone();
            (label, commands::dispatch(&cx, *command).await)
        }
        Some(InteractionData::MessageComponent(component)) => {
            let label = component.custom_id.split(':').next().unwrap_or_default().to_owned();
            (
                format!("component:{label}"),
                components::dispatch(&cx, &component.custom_id, &component.values).await,
            )
        }
        _ => return,
    };
    let outcome = if result.is_ok() { "ok" } else { "error" };
    metrics::counter!("pinchora_interactions_total", "name" => label.clone(), "outcome" => outcome).increment(1);
    metrics::histogram!("pinchora_interaction_seconds", "name" => label.clone())
        .record(started.elapsed().as_secs_f64());
    if let Err(error) = result {
        tracing::error!(name = %label, error = ?error, "interaction failed");
        let _ = cx
            .reply_ephemeral("Something went wrong on our side. Please try again in a moment.")
            .await;
    }
}
