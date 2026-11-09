/// Reads the StatefulSet ordinal from a pod hostname: `pinchora-gateway-3` → 3.
pub fn ordinal_from_hostname(hostname: &str) -> Option<u16> {
    hostname.rsplit_once('-')?.1.parse().ok()
}

/// The cluster this process runs: the configured one, else the pod ordinal, else 0.
pub fn resolve_cluster_id(configured: Option<u16>) -> u16 {
    configured
        .or_else(|| {
            std::env::var("HOSTNAME")
                .ok()
                .as_deref()
                .and_then(ordinal_from_hostname)
        })
        .unwrap_or(0)
}

/// Rounds a shard count up to a multiple of `max_concurrency`, so every identify
/// bucket is full and clusters split evenly.
pub fn round_shards(shards: u32, max_concurrency: u16) -> u32 {
    let step = u32::from(max_concurrency.max(1));
    shards.max(1).div_ceil(step) * step
}

#[cfg(test)]
mod tests {
    use crate::infra::cluster::*;

    #[test]
    fn hostname_ordinal() {
        assert_eq!(ordinal_from_hostname("pinchora-gateway-12"), Some(12));
        assert_eq!(ordinal_from_hostname("gateway-0"), Some(0));
        assert_eq!(ordinal_from_hostname("localhost"), None);
        assert_eq!(ordinal_from_hostname("pinchora-gateway-abc"), None);
    }

    #[test]
    fn shard_rounding() {
        assert_eq!(round_shards(1, 1), 1);
        assert_eq!(round_shards(0, 1), 1);
        assert_eq!(round_shards(17, 16), 32);
        assert_eq!(round_shards(32, 16), 32);
    }
}
