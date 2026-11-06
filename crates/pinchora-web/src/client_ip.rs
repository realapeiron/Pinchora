//! Who a request comes from, when reverse proxies sit in front of the dashboard: the connection's
//! address, unless that is a proxy we trust, which may then name the client in `X-Forwarded-For`.

use std::net::{IpAddr, SocketAddr};

use anyhow::Context as _;
use axum::http::HeaderMap;

/// Addresses and ranges allowed to say who the client is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrustedProxies(Vec<(IpAddr, u8)>);

impl TrustedProxies {
    /// Comma-separated addresses (`10.0.0.1`) and CIDR ranges (`172.16.0.0/12`, `fd00::/8`).
    pub fn parse(list: &str) -> anyhow::Result<Self> {
        let mut ranges = Vec::new();
        for entry in list.split(',').map(str::trim).filter(|e| !e.is_empty()) {
            let (address, bits) = match entry.split_once('/') {
                Some((address, bits)) => (address, Some(bits)),
                None => (entry, None),
            };
            let address: IpAddr = address
                .parse()
                .with_context(|| format!("PINCHORA_WEB__TRUSTED_PROXIES: {entry:?} is not an address"))?;
            let address = address.to_canonical();
            let max = if address.is_ipv4() { 32 } else { 128 };
            let bits = match bits {
                Some(bits) => bits
                    .parse::<u8>()
                    .ok()
                    .filter(|b| *b <= max)
                    .with_context(|| format!("PINCHORA_WEB__TRUSTED_PROXIES: {entry:?} has a bad prefix length"))?,
                None => max,
            };
            ranges.push((address, bits));
        }
        Ok(Self(ranges))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
        let ip = ip.to_canonical();
        self.0.iter().any(|&(range, bits)| in_range(ip, range, bits))
    }

    /// The client behind a connection from `peer`. A trusted peer's `X-Forwarded-For` is read from
    /// the right, skipping the proxies we trust: the first address that is not one is the client.
    /// Anything a client wrote further left is ignored, and so is the header from untrusted peers.
    pub fn client(&self, peer: IpAddr, headers: &HeaderMap) -> IpAddr {
        let mut client = peer.to_canonical();
        if !self.contains(client) {
            return client;
        }
        let hops: Vec<&str> = headers
            .get_all("x-forwarded-for")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(','))
            .map(str::trim)
            .collect();
        for hop in hops.into_iter().rev() {
            let Some(address) = parse_hop(hop) else {
                // A hop we cannot read: the last trusted proxy is the best we know.
                return client;
            };
            client = address;
            if !self.contains(address) {
                break;
            }
        }
        client
    }
}

/// `1.2.3.4`, `1.2.3.4:5678`, `2001:db8::1` or `[2001:db8::1]:443`.
fn parse_hop(hop: &str) -> Option<IpAddr> {
    hop.parse::<IpAddr>()
        .or_else(|_| hop.parse::<SocketAddr>().map(|s| s.ip()))
        .ok()
        .map(|ip| ip.to_canonical())
}

fn in_range(ip: IpAddr, range: IpAddr, bits: u8) -> bool {
    match (ip, range) {
        (IpAddr::V4(ip), IpAddr::V4(range)) => {
            let mask = u32::MAX.checked_shl(32 - u32::from(bits)).unwrap_or(0);
            u32::from(ip) & mask == u32::from(range) & mask
        }
        (IpAddr::V6(ip), IpAddr::V6(range)) => {
            let mask = u128::MAX.checked_shl(128 - u32::from(bits)).unwrap_or(0);
            u128::from(ip) & mask == u128::from(range) & mask
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    fn forwarded(values: &[&str]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for value in values {
            headers.append("x-forwarded-for", HeaderValue::from_str(value).unwrap());
        }
        headers
    }

    #[test]
    fn lists_parse_addresses_and_ranges() {
        let list = TrustedProxies::parse(" 172.16.0.0/12, 10.0.0.1 ,fd00::/8,").unwrap();
        assert!(list.contains(ip("172.20.3.4")) && !list.contains(ip("172.32.0.1")));
        assert!(list.contains(ip("10.0.0.1")) && !list.contains(ip("10.0.0.2")));
        assert!(list.contains(ip("fd12::1")) && !list.contains(ip("fe80::1")));
        assert!(list.contains(ip("::ffff:10.0.0.1")), "IPv4 seen through IPv6");
        assert!(TrustedProxies::parse("").unwrap().is_empty());
        assert!(TrustedProxies::parse("0.0.0.0/0").unwrap().contains(ip("8.8.8.8")));
        for bad in ["proxy", "10.0.0.0/33", "fd00::/129", "10.0.0.0/x"] {
            assert!(TrustedProxies::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn only_trusted_proxies_name_the_client() {
        let list = TrustedProxies::parse("172.16.0.0/12").unwrap();
        let header = forwarded(&["203.0.113.9"]);
        assert_eq!(list.client(ip("172.18.0.5"), &header), ip("203.0.113.9"));
        assert_eq!(
            list.client(ip("198.51.100.7"), &header),
            ip("198.51.100.7"),
            "a client cannot pick its own address"
        );
        assert_eq!(
            TrustedProxies::default().client(ip("172.18.0.5"), &header),
            ip("172.18.0.5"),
            "nobody is trusted by default"
        );
        assert_eq!(
            list.client(ip("172.18.0.5"), &HeaderMap::new()),
            ip("172.18.0.5"),
            "a proxy without the header is the client"
        );
    }

    #[test]
    fn the_rightmost_untrusted_hop_wins() {
        let list = TrustedProxies::parse("10.0.0.0/8").unwrap();
        // The client claimed 1.1.1.1; the edge proxy appended what it saw, then an inner proxy.
        let header = forwarded(&["1.1.1.1, 203.0.113.9", "10.1.1.1"]);
        assert_eq!(list.client(ip("10.2.2.2"), &header), ip("203.0.113.9"));
        let ported = forwarded(&["203.0.113.9:5555"]);
        assert_eq!(list.client(ip("10.2.2.2"), &ported), ip("203.0.113.9"));
        let garbage = forwarded(&["203.0.113.9, nonsense"]);
        assert_eq!(list.client(ip("10.2.2.2"), &garbage), ip("10.2.2.2"));
        let all_proxies = forwarded(&["10.9.9.9"]);
        assert_eq!(list.client(ip("10.2.2.2"), &all_proxies), ip("10.9.9.9"));
    }
}
