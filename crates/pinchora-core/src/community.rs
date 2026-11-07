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
