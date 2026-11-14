use crate::community::tickets::open::*;
#[test]
fn support_hours_follow_local_weekdays_and_both_dst_transitions() {
    let settings = pinchora_db::TicketOperationSettings {
        support_timezone: "America/New_York".into(),
        support_hours: vec![pinchora_db::SupportWindow {
            day: 7,
            start: 60,
            end: 120,
        }],
        ..Default::default()
    };
    for utc in ["2026-11-01T05:30:00Z", "2026-11-01T06:30:00Z", "2026-03-08T06:30:00Z"] {
        assert!(!outside_support_hours(&settings, utc.parse().unwrap()), "{utc}");
    }
    for utc in ["2026-11-01T07:00:00Z", "2026-03-08T07:00:00Z", "2026-11-02T06:30:00Z"] {
        assert!(outside_support_hours(&settings, utc.parse().unwrap()), "{utc}");
    }
    let settings = pinchora_db::TicketOperationSettings {
        support_timezone: "Europe/Istanbul".into(),
        support_hours: vec![pinchora_db::SupportWindow {
            day: 1,
            start: 0,
            end: 60,
        }],
        ..settings
    };
    assert!(!outside_support_hours(
        &settings,
        "2026-11-08T21:30:00Z".parse().unwrap()
    ));
}
