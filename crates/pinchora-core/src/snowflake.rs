use chrono::{DateTime, Utc};

const DISCORD_EPOCH_MS: i64 = 1_420_070_400_000;

/// Creation time encoded in a Discord snowflake.
pub fn timestamp(id: u64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis((id >> 22) as i64 + DISCORD_EPOCH_MS).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #[test]
    fn decodes() {
        // Example id from Discord's docs: 2016-04-30 11:18:25.796 UTC.
        let t = super::timestamp(175_928_847_299_117_063);
        assert_eq!(t.timestamp_millis(), 1_462_015_105_796);
    }
}
