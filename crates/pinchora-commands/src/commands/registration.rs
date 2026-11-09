//! Typed command factories and their matching handlers. Compatibility metadata is compiled by xtask.
use std::{future::Future, pin::Pin, sync::LazyLock};

use twilight_model::application::{command::Command, interaction::application_command::CommandData};

use crate::Cx;

pub(crate) type Handler =
    for<'a> fn(&'a Cx, CommandData) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>>;

#[derive(Clone, Copy)]
pub(crate) struct CommandEntry {
    pub key: &'static str,
    pub order: usize,
    pub name: &'static str,
    pub definition: fn() -> Command,
    pub dispatch: Handler,
}

macro_rules! commands {
    (typed { $($ty:ty,)* }
     context { $(( $name:literal, $($module:ident)::+),)* }) => {
        pub(crate) const COMMANDS: &[$crate::commands::registration::CommandEntry] = &[
                $(
                    $crate::commands::registration::CommandEntry {
                        key: stringify!($ty),
                        order: 0,
                        name: <$ty as twilight_interactions::command::CreateCommand>::NAME,
                        definition: || <$ty as twilight_interactions::command::CreateCommand>::create_command().into(),
                        dispatch: |cx, data| Box::pin(async move {
                            <$ty as twilight_interactions::command::CommandModel>::from_interaction(data.into())?.run(cx).await
                        }),
                    },
                )*
                $(
                    $crate::commands::registration::CommandEntry {
                        key: $name,
                        order: 0,
                        name: $name,
                        definition: $($module)::+::definition,
                        dispatch: |cx, data| Box::pin($($module)::+::run(cx, data)),
                    },
                )*
            ];
    };
}
pub(crate) use commands;

// The checked generator consumes these tokens and emits the compatibility tables used at runtime.
macro_rules! compatibility {
    ($($section:ident { $($entry:tt)* })*) => {};
}
pub(crate) use compatibility;

include!("registration/command_order.rs");

fn command_order(key: &str) -> usize {
    let key: String = key.chars().filter(|c| !c.is_whitespace()).collect();
    COMMAND_ORDER
        .iter()
        .position(|candidate| *candidate == key)
        .unwrap_or(usize::MAX)
}

pub(crate) fn ordered(entries: &[CommandEntry]) -> impl Iterator<Item = CommandEntry> + '_ {
    entries.iter().copied().map(|mut entry| {
        entry.order = command_order(entry.key);
        entry
    })
}

macro_rules! domains {
    ($($domain:ident),* $(,)?) => {
        $(pub(crate) mod $domain;)*

        pub(crate) fn entries() -> &'static [$crate::commands::registration::CommandEntry] {
            static ENTRIES: std::sync::LazyLock<Vec<$crate::commands::registration::CommandEntry>> =
                std::sync::LazyLock::new(|| {
                    let mut entries = vec![];
                    $(entries.extend($crate::commands::registration::ordered($domain::registration::COMMANDS));)*
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
pub(crate) mod fixture;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod behavior;
