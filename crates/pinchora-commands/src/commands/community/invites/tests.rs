    use crate::commands::community::invites::*;
    #[test]
    fn age_boundary_and_zero_window() {
        let now = Utc.timestamp_millis_opt(1_700_000_000_000).single().unwrap();
        let snowflake =
            |age_days: i64| (((now.timestamp_millis() - age_days * 86_400_000 - 1_420_070_400_000) as u64) << 22) | 1;
        assert!(is_fake_account(snowflake(6), 7, now));
        assert!(!is_fake_account(snowflake(7), 7, now));
        assert!(!is_fake_account(snowflake(0), 0, now));
    }
