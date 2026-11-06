//! The rules of the roles pack that need neither Discord nor the database: what a pick on a role panel
//! does, how people write a delay, and the limits. The bot and the dashboard decide with these.

/// Auto roles one server can have.
pub const MAX_AUTO_ROLES: usize = 10;
/// Colour roles one server can have.
pub const MAX_COLOR_ROLES: usize = 32;
/// Roles one server can name as the only ones allowed to pick a colour.
pub const MAX_COLOR_ALLOWED: usize = 10;
/// The longest an auto role waits before it is given: a month.
pub const MAX_DELAY_SECS: i64 = 30 * 86_400;
/// The longest a role lasts before it is taken away, and the shortest: a year, and a minute.
pub const MAX_EXPIRY_SECS: i64 = 365 * 86_400;
pub const MIN_EXPIRY_SECS: i64 = 60;
/// How long the roles of a member who left are kept for them.
pub const STICKY_KEPT_DAYS: i64 = 60;

/// Who an auto role is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AutoKind {
    /// A human who joins.
    Join,
    /// A bot that joins.
    Bot,
    /// A member who passes the join gate.
    AfterVerify,
}

impl AutoKind {
    pub const ALL: [Self; 3] = [Self::Join, Self::Bot, Self::AfterVerify];

    /// The word stored in the database and used in the API.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Join => "join",
            Self::Bot => "bot",
            Self::AfterVerify => "after_verify",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.key() == key)
    }
}

/// How a role panel is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PanelStyle {
    /// A button per role.
    #[default]
    Buttons,
    /// One menu to pick from.
    Select,
    /// An emoji per role under the message.
    Reactions,
}

impl PanelStyle {
    pub const ALL: [Self; 3] = [Self::Buttons, Self::Select, Self::Reactions];

    pub const fn key(self) -> &'static str {
        match self {
            Self::Buttons => "buttons",
            Self::Select => "select",
            Self::Reactions => "reactions",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.key() == key)
    }
}

/// How many roles a panel can hold: five rows of five buttons, or 25 options in a menu. Reactions
/// share the number.
pub const MAX_PANEL_ENTRIES: usize = 25;

/// What a panel asks of the picks a member makes on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PanelRules {
    /// At most one role of the panel at a time: picking another swaps.
    pub exclusive: bool,
    /// The fewest roles of the panel a member keeps (0: none needed).
    pub min_picks: usize,
    /// The most roles of the panel a member holds (0: no limit).
    pub max_picks: usize,
}

impl PanelRules {
    /// The most roles of the panel a member may hold, when there is a limit.
    pub fn cap(&self) -> Option<usize> {
        if self.exclusive {
            Some(1)
        } else {
            (self.max_picks > 0).then_some(self.max_picks)
        }
    }

    /// Whether the panel is only a toggle per role, as every panel was before the rules existed.
    pub fn is_plain(&self) -> bool {
        !self.exclusive && self.min_picks == 0 && self.max_picks == 0
    }
}

/// What is wrong with the rules a manager gave a panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RulesError {
    /// More than 25: a panel holds no more roles than that.
    TooMany,
    /// The fewest is above the most.
    MinAboveMax,
    /// One at a time cannot ask to keep more than one.
    ExclusiveMin,
}

/// Checks a panel's rules the way the command and the dashboard both want them.
pub fn check_rules(exclusive: bool, min_picks: usize, max_picks: usize) -> Result<(), RulesError> {
    if min_picks > MAX_PANEL_ENTRIES || max_picks > MAX_PANEL_ENTRIES {
        return Err(RulesError::TooMany);
    }
    if max_picks > 0 && min_picks > max_picks {
        return Err(RulesError::MinAboveMax);
    }
    if exclusive && min_picks > 1 {
        return Err(RulesError::ExclusiveMin);
    }
    Ok(())
}

/// The roles to give and to take away.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Changes {
    pub add: Vec<u64>,
    pub remove: Vec<u64>,
}

/// Why a pick changes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The role is not one of the panel's.
    NotOnPanel,
    /// The member would hold more than this many roles of the panel.
    TooMany(usize),
    /// The member would hold fewer than this many roles of the panel.
    TooFew(usize),
}

/// Which of `panel` the member holds.
fn held_of(panel: &[u64], held: &[u64]) -> Vec<u64> {
    panel.iter().copied().filter(|r| held.contains(r)).collect()
}

/// A click on one role of a panel: gives it, or takes it back when the member holds it. An exclusive
/// panel swaps the member's other role of it away; a cap refuses the role past it, and a floor refuses
/// to take the last one.
pub fn toggle(rules: &PanelRules, panel: &[u64], held: &[u64], role: u64) -> Result<Changes, Refusal> {
    if !panel.contains(&role) {
        return Err(Refusal::NotOnPanel);
    }
    let mine = held_of(panel, held);
    if mine.contains(&role) {
        if rules.min_picks > 0 && mine.len() <= rules.min_picks {
            return Err(Refusal::TooFew(rules.min_picks));
        }
        return Ok(Changes {
            add: Vec::new(),
            remove: vec![role],
        });
    }
    if rules.exclusive {
        return Ok(Changes {
            add: vec![role],
            remove: mine,
        });
    }
    if let Some(cap) = rules.cap()
        && mine.len() >= cap
    {
        return Err(Refusal::TooMany(cap));
    }
    Ok(Changes {
        add: vec![role],
        remove: Vec::new(),
    })
}

/// A pick from a menu: `chosen` is what the member wants to hold of the panel, so the roles they hold
/// that they did not choose are taken away.
pub fn choose(rules: &PanelRules, panel: &[u64], held: &[u64], chosen: &[u64]) -> Result<Changes, Refusal> {
    let mut wanted: Vec<u64> = Vec::with_capacity(chosen.len());
    for role in chosen {
        if !panel.contains(role) {
            return Err(Refusal::NotOnPanel);
        }
        if !wanted.contains(role) {
            wanted.push(*role);
        }
    }
    if let Some(cap) = rules.cap()
        && wanted.len() > cap
    {
        return Err(Refusal::TooMany(cap));
    }
    if wanted.len() < rules.min_picks {
        return Err(Refusal::TooFew(rules.min_picks));
    }
    let mine = held_of(panel, held);
    Ok(Changes {
        add: wanted.iter().copied().filter(|r| !mine.contains(r)).collect(),
        remove: mine.into_iter().filter(|r| !wanted.contains(r)).collect(),
    })
}

/// Why a written time was not taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanError {
    /// Not a time like `45m`, `2h30m` or `3d`.
    Unreadable,
    /// Shorter than this many seconds.
    TooShort(i64),
    /// Longer than this many seconds.
    TooLong(i64),
}

/// A time as people write it: `45m`, `2h30m`, `1d 12h`, `90 minutes` (units `w d h m s`). `off`, `none`
/// and `0` are no time at all. The result is in seconds.
pub fn parse_span(text: &str) -> Result<i64, SpanError> {
    let text = text.trim().to_lowercase();
    if matches!(text.as_str(), "" | "0" | "off" | "none" | "never" | "no") {
        return Ok(0);
    }
    let mut total: i64 = 0;
    let mut chars = text.chars().peekable();
    let mut any = false;
    while chars.peek().is_some() {
        while chars.next_if(|c| c.is_whitespace() || *c == ',').is_some() {}
        let mut digits = String::new();
        while let Some(c) = chars.next_if(char::is_ascii_digit) {
            digits.push(c);
        }
        while !digits.is_empty() && chars.next_if(|c| c.is_whitespace()).is_some() {}
        let mut unit = String::new();
        while let Some(c) = chars.next_if(|c| c.is_ascii_alphabetic()) {
            unit.push(c);
        }
        if digits.is_empty() && unit.is_empty() && chars.peek().is_none() {
            break;
        }
        let amount: i64 = digits.parse().map_err(|_| SpanError::Unreadable)?;
        let unit_secs: i64 = match unit.as_str() {
            "w" | "wk" | "wks" | "week" | "weeks" => 7 * 86_400,
            "d" | "day" | "days" => 86_400,
            "h" | "hr" | "hrs" | "hour" | "hours" => 3_600,
            "m" | "min" | "mins" | "minute" | "minutes" => 60,
            "s" | "sec" | "secs" | "second" | "seconds" => 1,
            _ => return Err(SpanError::Unreadable),
        };
        total = amount
            .checked_mul(unit_secs)
            .and_then(|step| total.checked_add(step))
            .filter(|t| *t <= MAX_EXPIRY_SECS * 2)
            .ok_or(SpanError::TooLong(MAX_EXPIRY_SECS))?;
        any = true;
    }
    if any { Ok(total) } else { Err(SpanError::Unreadable) }
}

/// [`parse_span`] held to `min..=max` seconds; zero stays zero (no time).
pub fn parse_span_within(text: &str, min: i64, max: i64) -> Result<i64, SpanError> {
    let secs = parse_span(text)?;
    if secs == 0 {
        Ok(0)
    } else if secs < min {
        Err(SpanError::TooShort(min))
    } else if secs > max {
        Err(SpanError::TooLong(max))
    } else {
        Ok(secs)
    }
}

/// A time in a few words: `1d 12h`, `45m`, `30s`; `0` for none. Units are the ones people type.
pub fn format_span(secs: i64) -> String {
    if secs <= 0 {
        return "0".to_owned();
    }
    let (days, rest) = (secs / 86_400, secs % 86_400);
    let (hours, rest) = (rest / 3_600, rest % 3_600);
    let (minutes, seconds) = (rest / 60, rest % 60);
    let mut parts = Vec::new();
    for (n, unit) in [(days, "d"), (hours, "h"), (minutes, "m"), (seconds, "s")] {
        if n > 0 {
            parts.push(format!("{n}{unit}"));
        }
    }
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const PANEL: [u64; 3] = [10, 20, 30];

    #[test]
    fn keys_round_trip() {
        for kind in AutoKind::ALL {
            assert_eq!(AutoKind::parse(kind.key()), Some(kind));
        }
        for style in PanelStyle::ALL {
            assert_eq!(PanelStyle::parse(style.key()), Some(style));
        }
        assert_eq!(AutoKind::parse("nope"), None);
        assert_eq!(PanelStyle::default(), PanelStyle::Buttons);
    }

    #[test]
    fn a_plain_panel_toggles_one_role() {
        let rules = PanelRules::default();
        assert!(rules.is_plain());
        let give = toggle(&rules, &PANEL, &[], 10).unwrap();
        assert_eq!((give.add, give.remove), (vec![10], vec![]));
        let take = toggle(&rules, &PANEL, &[10, 99], 10).unwrap();
        assert_eq!((take.add, take.remove), (vec![], vec![10]));
        assert_eq!(toggle(&rules, &PANEL, &[], 99), Err(Refusal::NotOnPanel));
    }

    #[test]
    fn an_exclusive_panel_swaps() {
        let rules = PanelRules {
            exclusive: true,
            ..Default::default()
        };
        let swap = toggle(&rules, &PANEL, &[20, 99], 10).unwrap();
        assert_eq!((swap.add, swap.remove), (vec![10], vec![20]), "other roles stay");
        // Clicking the one already held takes it away, unless the panel needs one.
        let drop = toggle(&rules, &PANEL, &[20], 20).unwrap();
        assert_eq!((drop.add, drop.remove), (vec![], vec![20]));
        let keep = PanelRules { min_picks: 1, ..rules };
        assert_eq!(toggle(&keep, &PANEL, &[20], 20), Err(Refusal::TooFew(1)));
        assert!(!rules.is_plain());
    }

    #[test]
    fn a_cap_refuses_the_role_past_it() {
        let rules = PanelRules {
            max_picks: 2,
            ..Default::default()
        };
        assert!(toggle(&rules, &PANEL, &[10], 20).is_ok());
        assert_eq!(toggle(&rules, &PANEL, &[10, 20], 30), Err(Refusal::TooMany(2)));
        // Taking one back is always fine, and roles outside the panel do not count.
        assert!(toggle(&rules, &PANEL, &[10, 20], 10).is_ok());
        assert!(toggle(&rules, &PANEL, &[10, 99, 98], 20).is_ok());
    }

    #[test]
    fn a_menu_sets_what_the_member_holds_of_the_panel() {
        let rules = PanelRules::default();
        let set = choose(&rules, &PANEL, &[10, 99], &[20, 30]).unwrap();
        assert_eq!((set.add, set.remove), (vec![20, 30], vec![10]));
        // Nothing chosen takes them all off (when the panel lets it).
        let none = choose(&rules, &PANEL, &[10, 20], &[]).unwrap();
        assert_eq!((none.add, none.remove), (vec![], vec![10, 20]));
        // The same pick twice is one pick; a forged id is refused.
        assert_eq!(choose(&rules, &PANEL, &[], &[10, 10]).unwrap().add, vec![10]);
        assert_eq!(choose(&rules, &PANEL, &[], &[10, 77]), Err(Refusal::NotOnPanel));
    }

    #[test]
    fn a_menu_keeps_to_its_minimum_and_maximum() {
        let rules = PanelRules {
            min_picks: 1,
            max_picks: 2,
            ..Default::default()
        };
        assert_eq!(choose(&rules, &PANEL, &[], &[]), Err(Refusal::TooFew(1)));
        assert!(choose(&rules, &PANEL, &[], &[10, 20]).is_ok());
        assert_eq!(choose(&rules, &PANEL, &[], &[10, 20, 30]), Err(Refusal::TooMany(2)));
        let exclusive = PanelRules {
            exclusive: true,
            ..Default::default()
        };
        assert_eq!(choose(&exclusive, &PANEL, &[], &[10, 20]), Err(Refusal::TooMany(1)));
        assert_eq!(exclusive.cap(), Some(1));
    }

    #[test]
    fn rules_are_checked() {
        assert_eq!(check_rules(false, 0, 0), Ok(()));
        assert_eq!(check_rules(true, 1, 0), Ok(()));
        assert_eq!(check_rules(false, 2, 3), Ok(()));
        assert_eq!(check_rules(false, 3, 2), Err(RulesError::MinAboveMax));
        assert_eq!(check_rules(true, 2, 0), Err(RulesError::ExclusiveMin));
        assert_eq!(check_rules(false, 0, 26), Err(RulesError::TooMany));
        // No maximum means no ceiling to be above.
        assert_eq!(check_rules(false, 5, 0), Ok(()));
    }

    #[test]
    fn times_are_read_like_people_write_them() {
        assert_eq!(parse_span("45m"), Ok(2_700));
        assert_eq!(parse_span("2h30m"), Ok(9_000));
        assert_eq!(parse_span(" 1d 12H "), Ok(129_600));
        assert_eq!(parse_span("90 minutes"), Ok(5_400));
        assert_eq!(parse_span("1w"), Ok(604_800));
        assert_eq!(parse_span("30s"), Ok(30));
        for none in ["0", "off", "None", "", "  "] {
            assert_eq!(parse_span(none), Ok(0), "{none:?}");
        }
        for bad in ["soon", "30", "m5", "5x", "h", "2h30", "1.5h", "-5m"] {
            assert_eq!(parse_span(bad), Err(SpanError::Unreadable), "{bad:?}");
        }
        assert!(matches!(
            parse_span("99999999999999999999d"),
            Err(SpanError::Unreadable)
        ));
        assert!(matches!(parse_span("99999999d"), Err(SpanError::TooLong(_))));
    }

    #[test]
    fn times_are_held_to_their_bounds() {
        assert_eq!(parse_span_within("7d", MIN_EXPIRY_SECS, MAX_EXPIRY_SECS), Ok(604_800));
        assert_eq!(parse_span_within("off", MIN_EXPIRY_SECS, MAX_EXPIRY_SECS), Ok(0));
        assert_eq!(
            parse_span_within("30s", MIN_EXPIRY_SECS, MAX_EXPIRY_SECS),
            Err(SpanError::TooShort(MIN_EXPIRY_SECS))
        );
        assert_eq!(
            parse_span_within("2y", MIN_EXPIRY_SECS, MAX_EXPIRY_SECS),
            Err(SpanError::Unreadable)
        );
        assert_eq!(
            parse_span_within("400d", MIN_EXPIRY_SECS, MAX_EXPIRY_SECS),
            Err(SpanError::TooLong(MAX_EXPIRY_SECS))
        );
    }

    #[test]
    fn times_are_written_back_in_a_few_words() {
        assert_eq!(format_span(0), "0");
        assert_eq!(format_span(2_700), "45m");
        assert_eq!(format_span(129_600), "1d 12h");
        assert_eq!(format_span(90_061), "1d 1h 1m 1s");
        for secs in [60, 3_600, 86_400, 100_000] {
            assert_eq!(parse_span(&format_span(secs)), Ok(secs));
        }
    }
}
