//! Moderation's words in the reader's language: durations, the rungs of the warn ladder, refusals,
//! the names of the kinds of cases and of the permissions the commands need.

use chrono::Duration;
use pinchora_core::infra::i18n::Lang;
use pinchora_core::moderation::moderation::CaseKind;
use pinchora_core::moderation::moderation::Punishment;
use pinchora_core::moderation::moderation::Refusal;
use pinchora_core::moderation::moderation::Threshold;
use pinchora_core::t;

/// A duration the short way: `1w 2d`, `2h 30m`, `45s` (two units at most), as `format_duration` writes
/// it in English.
pub(crate) fn duration(lang: Lang, d: Duration) -> String {
    let mut left = d.num_seconds().max(0);
    let mut parts = Vec::new();
    for (unit, size) in [("w", 604_800), ("d", 86_400), ("h", 3_600), ("m", 60), ("s", 1)] {
        if left >= size {
            parts.push(t!(lang, &format!("mod.unit.{unit}"), n = left / size));
            left %= size;
        }
        if parts.len() == 2 {
            break;
        }
    }
    if parts.is_empty() {
        return t!(lang, "mod.unit.s", n = 0);
    }
    parts.join(" ")
}

/// "3 warns → 1h timeout".
pub(crate) fn rung(lang: Lang, rung: &Threshold) -> String {
    let what = match (rung.action, rung.duration) {
        (Punishment::Timeout, Some(d)) => t!(lang, "mod.rung.timeout-for", duration = duration(lang, d)),
        (Punishment::Timeout, None) => t!(lang, "mod.rung.timeout"),
        (Punishment::Ban, Some(d)) => t!(lang, "mod.rung.ban-for", duration = duration(lang, d)),
        (Punishment::Ban, None) => t!(lang, "mod.rung.ban"),
        (Punishment::Kick, _) => t!(lang, "mod.rung.kick"),
    };
    t!(lang, "mod.rung.line", count = rung.warns, what = what)
}

/// The sentence for a refusal; `verb` is a key of `[mod.verb]` (`ban`, `timeout`, ...).
pub(crate) fn refusal(lang: Lang, refusal: Refusal, verb: &str) -> String {
    let key = match refusal {
        Refusal::Yourself => "yourself",
        Refusal::Me => "me",
        Refusal::Owner => "owner",
        Refusal::Above => "above",
        Refusal::AboveMe => "above-me",
    };
    let verb = t!(lang, &format!("mod.verb.{verb}"));
    t!(lang, &format!("mod.refusal.{key}"), verb = verb)
}

/// The name of a kind of case ("Ban", "Warn removed"); an unknown kind keeps its key.
pub(crate) fn kind(lang: Lang, kind: Option<CaseKind>, key: &str) -> String {
    match kind {
        Some(kind) => t!(lang, &format!("mod.kind.{}", kind.key())),
        None => key.to_owned(),
    }
}

/// A permission the moderation commands need, by its key in `[mod.perm]`.
pub(crate) fn permission(lang: Lang, key: &str) -> String {
    t!(lang, &format!("mod.perm.{key}"))
}

#[cfg(test)]
mod tests {
    use crate::commands::moderation::words::*;

    #[test]
    fn durations_and_rungs_read_in_both_languages() {
        assert_eq!(duration(Lang::En, Duration::minutes(150)), "2h 30m");
        assert_eq!(duration(Lang::En, Duration::days(9) + Duration::hours(5)), "1w 2d");
        assert_eq!(duration(Lang::En, Duration::zero()), "0s");
        assert_eq!(duration(Lang::Tr, Duration::days(7)), "1 hafta");
        assert_eq!(duration(Lang::Tr, Duration::minutes(150)), "2 sa 30 dk");
        let timeout =
            pinchora_core::moderation::moderation::check_threshold(3, "timeout", Some(Duration::hours(1))).unwrap();
        assert_eq!(rung(Lang::En, &timeout), timeout.describe());
        assert_eq!(rung(Lang::Tr, &timeout), "3 uyarı → 1 sa susturma");
        let kick = pinchora_core::moderation::moderation::check_threshold(1, "kick", None).unwrap();
        assert_eq!(rung(Lang::En, &kick), "1 warn → kick");
    }

    #[test]
    fn refusals_name_what_was_tried() {
        assert_eq!(refusal(Lang::En, Refusal::Yourself, "ban"), "You can't ban yourself.");
        assert!(refusal(Lang::En, Refusal::AboveMe, "kick").contains("Drag my role"));
        assert!(refusal(Lang::En, Refusal::Above, "timeout").starts_with("You can't time out"));
        assert_eq!(
            refusal(Lang::Tr, Refusal::Owner, "ban"),
            "Sunucu sahibini yasaklamak mümkün değil."
        );
        for verb in ["ban", "kick", "timeout", "untimeout", "warn", "rename"] {
            assert!(!refusal(Lang::Tr, Refusal::Me, verb).contains("mod."), "{verb}");
        }
        for kind in CaseKind::ALL {
            assert!(
                !crate::commands::moderation::words::tests::kind(Lang::Tr, Some(kind), kind.key()).contains("mod.")
            );
        }
    }
}
