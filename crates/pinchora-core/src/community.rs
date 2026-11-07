//! Pure rules shared by the gateway, dashboard, and simulator.

/// A counting message is one nonnegative integer or two small integers with a basic operator.
pub fn count_value(text: &str, math: bool) -> Option<i64> {
    let tokens: Vec<_> = text.split_ascii_whitespace().collect();
    match tokens.as_slice() {
        [number] if number.bytes().all(|b| b.is_ascii_digit()) && number.len() <= 12 => number.parse().ok(),
        [left, op @ ("+" | "-" | "*" | "/"), right] if math => {
            if left.len() > 6
                || right.len() > 6
                || !left.bytes().all(|b| b.is_ascii_digit())
                || !right.bytes().all(|b| b.is_ascii_digit())
            {
                return None;
            }
            let (a, b) = (left.parse::<i64>().ok()?, right.parse::<i64>().ok()?);
            match *op {
                "+" => a.checked_add(b),
                "-" => a.checked_sub(b),
                "*" => a.checked_mul(b),
                "/" if b != 0 && a % b == 0 => Some(a / b),
                _ => None,
            }
        }
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountResult {
    Correct,
    Wrong,
    SameUser,
    Ignored,
}

pub fn judge_count(text: &str, math: bool, current: i64, last_user: Option<u64>, user: u64) -> CountResult {
    let Some(value) = count_value(text, math) else {
        return CountResult::Ignored;
    };
    if last_user == Some(user) {
        return CountResult::SameUser;
    }
    if current.checked_add(1) == Some(value) {
        CountResult::Correct
    } else {
        CountResult::Wrong
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailRoute {
    None,
    One(u64),
    Choose,
}
pub fn mail_route(guilds: &[u64]) -> MailRoute {
    match guilds {
        [] => MailRoute::None,
        [guild] => MailRoute::One(*guild),
        _ => MailRoute::Choose,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmissionStatus {
    Pending,
    Accepted,
    Denied,
}
impl SubmissionStatus {
    pub fn decide(self, accept: bool) -> Option<Self> {
        match self {
            Self::Pending => Some(if accept { Self::Accepted } else { Self::Denied }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counting_rejects_non_integer_and_long_expressions() {
        for input in ["1e9", "1.0", "1 + 2 + 3", "1 / 0", "7 / 2", "-1", "9999999999999"] {
            assert_eq!(count_value(input, true), None, "{input}");
        }
        assert_eq!(count_value("4 * 5", true), Some(20));
        assert_eq!(count_value("4 * 5", false), None);
        assert_eq!(judge_count("2", false, 1, Some(7), 7), CountResult::SameUser);
    }
    #[test]
    fn modmail_routes_by_shared_server_count() {
        assert_eq!(mail_route(&[]), MailRoute::None);
        assert_eq!(mail_route(&[7]), MailRoute::One(7));
        assert_eq!(mail_route(&[7, 8]), MailRoute::Choose);
    }
    #[test]
    fn forms_can_only_be_decided_once() {
        assert_eq!(SubmissionStatus::Pending.decide(true), Some(SubmissionStatus::Accepted));
        assert_eq!(SubmissionStatus::Accepted.decide(false), None);
    }
}

/// The name is also a Discord modal title; keep every writer within its 45-character limit.
pub fn valid_form(name: &str, questions: &serde_json::Value) -> bool {
    !name.trim().is_empty()
        && name.chars().count() <= 45
        && questions.as_array().is_some_and(|items| {
            (1..=5).contains(&items.len())
                && items.iter().all(|q| {
                    q.as_str()
                        .is_some_and(|q| !q.trim().is_empty() && q.chars().count() <= 45)
                })
        })
}

/// Discord modal fields are identified by custom ID, never by client-supplied ordering.
pub fn form_answers(fields: &[(String, String)]) -> Option<serde_json::Value> {
    if !(1..=5).contains(&fields.len()) {
        return None;
    }
    let mut answers = Vec::new();
    for index in 0..fields.len() {
        let key = format!("a{index}");
        let mut matches = fields.iter().filter(|(id, _)| id == &key);
        let (_, value) = matches.next()?;
        if matches.next().is_some() {
            return None;
        }
        answers.push(value);
    }
    Some(serde_json::json!(answers))
}

#[cfg(test)]
mod form_tests {
    use super::*;
    #[test]
    fn modal_limits_and_field_identity() {
        assert!(valid_form(&"a".repeat(45), &serde_json::json!(vec!["Why?"; 5])));
        for (name, questions) in [
            ("a".repeat(46), serde_json::json!(["Why?"])),
            (" ".into(), serde_json::json!(["Why?"])),
            ("Form".into(), serde_json::json!([])),
            ("Form".into(), serde_json::json!(vec!["Why?"; 6])),
            ("Form".into(), serde_json::json!(["a".repeat(46)])),
        ] {
            assert!(!valid_form(&name, &questions));
        }
        assert_eq!(
            form_answers(&[("a1".into(), "second".into()), ("a0".into(), "first".into())]),
            Some(serde_json::json!(["first", "second"]))
        );
        assert!(form_answers(&[("a0".into(), "a".into()), ("a0".into(), "b".into())]).is_none());
        assert!(form_answers(&[("forged".into(), "a".into())]).is_none());
    }
}
