mod registration;

registration::domains!(
    admin,
    community,
    economy,
    games,
    house,
    integrations,
    moderation,
    progression,
    roles,
    social,
);

use crate::Cx;
use crate::admin::undo;
pub use crate::commands::economy::legacy::{wallet_card_for, wallet_preview_for};
pub(crate) use registration::legacy_definitions;
use twilight_model::application::{command::Command, interaction::application_command::CommandData};

pub fn definitions() -> Vec<Command> {
    let mut commands = legacy_definitions();
    crate::infra::routing::extend(&mut commands);
    crate::infra::routing::remove_replaced(&mut commands);
    crate::infra::lang::localize(&mut commands);
    commands
}

pub(crate) async fn dispatch(cx: &Cx, data: CommandData) -> anyhow::Result<()> {
    if crate::infra::routing::is_admin_path(crate::infra::routing::canonical(&crate::admin::command_control::path(
        &data,
    ))) {
        if cx.guild_id().is_none() {
            return cx
                .reply_ephemeral(pinchora_core::t!(cx.lang(), "config.guild-only"))
                .await;
        }
        if !cx.can_manage_guild() {
            return cx
                .reply_ephemeral(pinchora_core::t!(cx.lang(), "config.needs-manage-server"))
                .await;
        }
    }
    let data = crate::infra::routing::adapt(data)?;
    if crate::infra::gate::refuses_command(cx, &data).await? {
        return Ok(());
    }
    if let Some(entry) = entries().iter().find(|entry| entry.name == data.name) {
        return (entry.dispatch)(cx, data).await;
    }
    anyhow::bail!("unknown command `{}`", data.name)
}

#[cfg(test)]
mod quick_command_budget {
    use crate::commands::*;

    #[test]
    fn quick_games_keep_the_slash_command_budget() {
        let definitions = definitions();
        assert_eq!(
            definitions.len(),
            crate::infra::routing::PROJECTED_ROOTS,
            "Discord's 100-command budget must not grow"
        );
        for (name, expected) in [
            (
                "play",
                [
                    "memory",
                    "minesweeper",
                    "higherlower",
                    "anagram",
                    "typing",
                    "guessnumber",
                ]
                .as_slice(),
            ),
            ("duel", ["othello"].as_slice()),
            ("party", ["nhie", "fun"].as_slice()),
        ] {
            let command = definitions.iter().find(|c| c.name == name).unwrap();
            assert!(
                command.options.len() <= 25,
                "/{name} has {} subcommands",
                command.options.len()
            );
            for sub in expected {
                assert!(command.options.iter().any(|o| o.name == *sub), "/{name} {sub} missing");
            }
        }
    }
}
