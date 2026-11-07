//! Deferring an interaction that outlasts Discord's three-second window.
//!
//! Over the HTTP endpoint the endpoint's own budget does it ([`Responder::defer_on_timeout`]). Over
//! the gateway nothing answers for a slow handler, so [`serve`] races it against [`DEFER_AFTER`] and
//! defers through REST; the handler's answer then lands as an edit of the original message.

use std::{future::Future, time::Duration};

use twilight_model::application::interaction::{
    Interaction, InteractionData, InteractionType,
    application_command::{CommandData, CommandOptionValue},
};

use crate::Responder;

/// Discord wants the first answer within 3 s, and the defer is a REST call that needs its own margin.
pub(crate) const DEFER_AFTER: Duration = Duration::from_millis(1_800);

/// What the deferral timer does with an interaction that is still unanswered when it fires.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deferral {
    /// The handler may open a modal, which only works as the first answer: never defer.
    Never,
    /// Defer. A command's answer becomes a public message; a component's answer edits the clicked one.
    Public,
    /// Defer privately: a command that answers privately, because an edit cannot make a public message private.
    Private,
}

/// Slash commands that answer privately, as `name`, `name sub` or `name group sub`; an entry covers
/// everything below it. What counts is the first answer: refusals are private nearly everywhere, so
/// a command whose success is public stays out. A unit test checks that every entry names a real
/// command, and the simulator that nothing listed here ever answers publicly.
const PRIVATE: &[&str] = &[
    "autoroles",
    "beachcomb",
    "bio",
    "case",
    "character",
    "color",
    "config",
    "counters",
    "crabpot",
    "customcommand delete",
    "customcommand info",
    "customcommand list",
    "customize",
    "dex",
    "feeds",
    "event list",
    "event attendees",
    "event remind",
    "event template",
    "event edit",
    "event cancel",
    "fish gear",
    "fish sell",
    "fish shop",
    "friends remove",
    "gift",
    "giveaway",
    "help",
    "house contest vote",
    "invites temp",
    "inviteadmin",
    "house decorate",
    "house edit",
    "house lock",
    "house upgrade",
    "nick",
    "language",
    "partner leave",
    "party stop",
    "pet adopt",
    "pet list",
    "pet rename",
    "plaza donate",
    "plaza edit",
    "play picross",
    "play wordle",
    "play 2048",
    "poll",
    "privacy",
    "purge",
    "quests",
    "reason",
    "remind",
    "role",
    "roles",
    "stall manage",
    "stall sell",
    "stats",
    "suggest",
    "suggestion",
    "warnings",
    "ticket",
    "tickets",
    "automod",
    "voice",
    "xp",
    "info channel",
    "info invite",
    "starboard",
    "verify",
    "verification",
    "raid",
];

/// Slash commands that answer with a form, which only works as the first answer: never deferred.
const OPENS_FORM: &[&str] = &["customcommand create", "customcommand edit", "event create"];

/// Component clicks that may answer with a modal, as `prefix:action` of the custom id. The simulator
/// checks that every modal it sees opened comes from one of these.
const OPENS_MODAL: &[&str] = &[
    "ev:c", "ca:hex", "ca:name", "gb:s", "pz:m", "wo:g", "cs:h", "hl:w", "ac:w", "cn:w", "wx:g", "tk:o", "tk:s",
    "vc:lm", "vc:rn", "cq:a",
];

impl Deferral {
    pub(crate) fn of(interaction: &Interaction) -> Self {
        // Suggestions are the only answer an autocomplete takes: a defer would break it.
        if interaction.kind == InteractionType::ApplicationCommandAutocomplete {
            return Self::Never;
        }
        match &interaction.data {
            Some(InteractionData::ApplicationCommand(command)) if covers(OPENS_FORM, &command_path(command), ' ') => {
                Self::Never
            }
            Some(InteractionData::ApplicationCommand(command)) if covers(PRIVATE, &command_path(command), ' ') => {
                Self::Private
            }
            Some(InteractionData::MessageComponent(component)) if covers(OPENS_MODAL, &component.custom_id, ':') => {
                Self::Never
            }
            _ => Self::Public,
        }
    }
}

/// Whether `key` is one of `entries` or sits below one, `sep` separating the levels.
fn covers(entries: &[&str], key: &str, sep: char) -> bool {
    entries
        .iter()
        .any(|entry| key == *entry || key.strip_prefix(entry).is_some_and(|rest| rest.starts_with(sep)))
}

/// `house edit` for `/house edit`: the command's name and the subcommand levels it was invoked through.
fn command_path(command: &CommandData) -> String {
    let mut path = command.name.clone();
    let mut options = &command.options;
    while let Some(option) = options.first() {
        let (CommandOptionValue::SubCommand(inner) | CommandOptionValue::SubCommandGroup(inner)) = &option.value else {
            break;
        };
        path.push(' ');
        path.push_str(&option.name);
        options = inner;
    }
    path
}

/// Gateway mode: runs `handler`, and defers through REST if it is still going after `budget`.
pub(crate) async fn defer_slow<H: Future<Output = ()>>(responder: &Responder, budget: Duration, handler: H) {
    let defer = async {
        match responder.defer_if_pending().await {
            Ok(true) => metrics::counter!("pinchora_interactions_deferred_total").increment(1),
            Ok(false) => {}
            Err(error) => tracing::warn!(%error, "could not defer a slow interaction"),
        }
    };
    serve(budget, handler, defer).await;
}

/// Runs `handler`. If it is still going after `budget`, runs `defer` alongside it, once; the handler
/// is polled throughout, because it may be holding the very lock `defer` waits for.
async fn serve<H, D>(budget: Duration, handler: H, defer: D)
where
    H: Future<Output = ()>,
    D: Future<Output = ()>,
{
    tokio::pin!(handler);
    if tokio::time::timeout(budget, handler.as_mut()).await.is_ok() {
        return;
    }
    tokio::join!(handler, defer);
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use tokio::{sync::Mutex, time::sleep};
    use twilight_model::application::command::{Command, CommandOption, CommandOptionType};

    use super::*;

    /// Every command path the definitions register: `name`, `name sub`, `name group sub`.
    fn registered() -> Vec<String> {
        fn walk(prefix: &str, options: &[CommandOption], paths: &mut Vec<String>) {
            for option in options {
                if matches!(
                    option.kind,
                    CommandOptionType::SubCommand | CommandOptionType::SubCommandGroup
                ) {
                    let path = format!("{prefix} {}", option.name);
                    paths.push(path.clone());
                    walk(&path, option.options.as_deref().unwrap_or_default(), paths);
                }
            }
        }
        let mut paths = Vec::new();
        for Command { name, options, .. } in crate::command_definitions() {
            paths.push(name.clone());
            walk(&name, &options, &mut paths);
        }
        paths
    }

    #[test]
    fn every_private_entry_names_a_command() {
        let paths = registered();
        for entry in PRIVATE {
            assert!(
                paths.iter().any(|path| covers(&[entry], path, ' ')),
                "`{entry}` is not a command, so it can never match"
            );
        }
    }

    #[test]
    fn every_form_entry_names_a_command_and_is_not_also_private() {
        let paths = registered();
        for entry in OPENS_FORM {
            assert!(
                paths.iter().any(|path| covers(&[entry], path, ' ')),
                "`{entry}` is not a command, so it can never match"
            );
            assert!(
                !PRIVATE
                    .iter()
                    .any(|p| covers(&[p], entry, ' ') || covers(&[entry], p, ' ')),
                "`{entry}` opens a form, so it cannot be deferred privately"
            );
        }
    }

    #[test]
    fn private_entries_are_not_repeated_or_nested() {
        for (n, entry) in PRIVATE.iter().enumerate() {
            for other in &PRIVATE[n + 1..] {
                assert!(
                    !covers(&[entry], other, ' ') && !covers(&[other], entry, ' '),
                    "`{entry}` and `{other}` overlap"
                );
            }
        }
    }

    #[test]
    fn entries_cover_whole_levels() {
        assert!(covers(&["house edit"], "house edit", ' '));
        assert!(covers(&["config"], "config level-roles add", ' '));
        assert!(!covers(&["house"], "household", ' '));
        assert!(!covers(&["house edit"], "house", ' '));
        assert!(covers(&["wo:g"], "wo:g:daily:5", ':'));
        assert!(!covers(&["wo:g"], "wo:gx", ':'));
    }

    /// The timer is the only thing that defers, and it must not leave a handler stuck behind a lock.
    #[tokio::test(start_paused = true)]
    async fn a_fast_handler_finishes_before_the_deferral_runs() {
        let deferred = Arc::new(AtomicUsize::new(0));
        let count = deferred.clone();
        serve(
            DEFER_AFTER,
            async { sleep(Duration::from_millis(100)).await },
            async move {
                count.fetch_add(1, Ordering::SeqCst);
            },
        )
        .await;
        assert_eq!(deferred.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn a_slow_handler_is_deferred_once_while_it_keeps_running() {
        let (started, deferred_at) = (tokio::time::Instant::now(), Arc::new(std::sync::Mutex::new(Vec::new())));
        let log = deferred_at.clone();
        serve(DEFER_AFTER, async { sleep(Duration::from_secs(5)).await }, async move {
            log.lock().unwrap().push(started.elapsed());
        })
        .await;
        assert_eq!(*deferred_at.lock().unwrap(), vec![DEFER_AFTER]);
        assert_eq!(started.elapsed(), Duration::from_secs(5), "the handler ran to its end");
    }

    /// The handler holds the lock when the timer fires, like `respond` in the middle of its REST call.
    #[tokio::test(start_paused = true)]
    async fn the_deferral_waits_for_a_handler_that_holds_its_lock() {
        let lock = Arc::new(Mutex::new(0));
        let order = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (handler_lock, handler_order) = (lock.clone(), order.clone());
        let (defer_lock, defer_order) = (lock.clone(), order.clone());
        serve(
            DEFER_AFTER,
            async move {
                let _held = handler_lock.lock().await;
                sleep(Duration::from_secs(3)).await;
                handler_order.lock().unwrap().push("handler");
            },
            async move {
                let _held = defer_lock.lock().await;
                defer_order.lock().unwrap().push("defer");
            },
        )
        .await;
        assert_eq!(*order.lock().unwrap(), ["handler", "defer"]);
    }
}
