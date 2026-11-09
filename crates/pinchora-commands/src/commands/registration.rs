//! Typed command factories and their matching handlers. Compatibility metadata is compiled by xtask.
use std::{future::Future, pin::Pin, sync::LazyLock};

use twilight_model::application::{command::Command, interaction::application_command::CommandData};

use crate::Cx;

pub(crate) type Handler =
    for<'a> fn(&'a Cx, CommandData) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>>;

pub(crate) struct CommandEntry {
    pub order: usize,
    pub name: &'static str,
    pub definition: fn() -> Command,
    pub dispatch: Handler,
}

macro_rules! commands {
    (typed { $($order:literal => $ty:ty,)* }
     context { $($context_order:literal => ($name:literal, $($module:ident)::+),)* }) => {
        pub(crate) fn commands() -> Vec<$crate::commands::registration::CommandEntry> {
            vec![
                $(
                    $crate::commands::registration::CommandEntry {
                        order: $order,
                        name: <$ty as twilight_interactions::command::CreateCommand>::NAME,
                        definition: || <$ty as twilight_interactions::command::CreateCommand>::create_command().into(),
                        dispatch: |cx, data| Box::pin(async move {
                            <$ty as twilight_interactions::command::CommandModel>::from_interaction(data.into())?.run(cx).await
                        }),
                    },
                )*
                $(
                    $crate::commands::registration::CommandEntry {
                        order: $context_order,
                        name: $name,
                        definition: $($module)::+::definition,
                        dispatch: |cx, data| Box::pin($($module)::+::run(cx, data)),
                    },
                )*
            ]
        }
    };
}
pub(crate) use commands;

// The checked generator consumes these tokens and emits the compatibility tables used at runtime.
macro_rules! compatibility {
    ($($section:ident { $($order:literal { $($entry:tt)* })* })*) => {};
}
pub(crate) use compatibility;

macro_rules! domains {
    ($($domain:ident),* $(,)?) => {
        $(pub(crate) mod $domain;)*

        pub(crate) fn entries() -> &'static [$crate::commands::registration::CommandEntry] {
            static ENTRIES: std::sync::LazyLock<Vec<$crate::commands::registration::CommandEntry>> =
                std::sync::LazyLock::new(|| {
                    let mut entries = vec![];
                    $(entries.extend($domain::registration::commands());)*
                    entries.sort_by_key(|entry| entry.order);
                    entries
                });
            &ENTRIES
        }
    };
}
pub(crate) use domains;

pub(crate) fn legacy_definitions() -> Vec<Command> {
    static COMMANDS: LazyLock<Vec<Command>> = LazyLock::new(|| {
        let mut commands = super::entries()
            .iter()
            .map(|entry| (entry.definition)())
            .collect::<Vec<_>>();
        crate::infra::lang::localize(&mut commands);
        commands
    });
    COMMANDS.clone()
}

#[cfg(test)]
mod fixture;
#[cfg(test)]
mod tests;
