//! Safe Minecraft Server List Ping and tracker data helpers.
use std::{net::IpAddr, time::Duration};

use anyhow::{Context, ensure};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpStream, lookup_host},
    time::timeout,
};

const MAX_PACKET: usize = 64 * 1024;
const DEADLINE: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct McStatus {
    pub version: String,
    pub online: u32,
    pub max: u32,
    pub motd: String,
    pub favicon: Option<String>,
}

/// Stable version identifiers in manifest order, used for news and idempotent release posts.
pub fn release_ids(manifest: &serde_json::Value) -> Vec<String> {
    manifest["versions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["type"] == "release")
        .filter_map(|item| item["id"].as_str())
        .take(20)
        .map(str::to_owned)
        .collect()
}

/// Decode the data URL used by SLP favicons without accepting arbitrary image formats or huge payloads.
pub fn decode_favicon(value: &str) -> Option<Vec<u8>> {
    let encoded = value.strip_prefix("data:image/png;base64,")?;
    if encoded.len() > 48_000 || encoded.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(encoded.len() / 4 * 3);
    for chunk in encoded.as_bytes().as_chunks::<4>().0 {
        let mut n = 0u32;
        let mut padding = 0;
        for (i, &byte) in chunk.iter().enumerate() {
            let digit = match byte {
                b'A'..=b'Z' => byte - b'A',
                b'a'..=b'z' => byte - b'a' + 26,
                b'0'..=b'9' => byte - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                b'=' if i >= 2 => {
                    padding += 1;
                    0
                }
                _ => return None,
            };
            if padding > 0 && byte != b'=' {
                return None;
            }
            n = (n << 6) | u32::from(digit);
        }
        out.push((n >> 16) as u8);
        if padding < 2 {
            out.push((n >> 8) as u8);
        }
        if padding == 0 {
            out.push(n as u8);
        }
    }
    (out.len() <= 36_000).then_some(out)
}

pub fn parse_host(input: &str) -> anyhow::Result<(String, u16)> {
    let input = input.trim();
    ensure!(
        !input.is_empty() && input.len() <= 253 && !input.contains(['/', '@', '#', '?', '\\']),
        "invalid host"
    );
    let (host, port) = match input.rsplit_once(':') {
        Some((host, port)) => (host, port.parse::<u16>().context("invalid port")?),
        None => (input, 25565),
    };
    let host = host.trim_end_matches('.');
    ensure!(
        port > 0
            && !host.is_empty()
            && host
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'-'),
        "invalid host or port"
    );
    Ok((host.to_ascii_lowercase(), port))
}

pub fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let a = ip.octets();
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_multicast()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || a[0] == 0
                || a[0] >= 224
                || (a[0] == 100 && (64..=127).contains(&a[1]))
                || (a[0] == 198 && (a[1] == 18 || a[1] == 19))
                || (a[0] == 192 && a[1] == 0 && a[2] == 0)
                || (a[0] == 192 && a[1] == 0 && a[2] == 2)
                || (a[0] == 198 && a[1] == 51 && a[2] == 100)
                || (a[0] == 203 && a[1] == 0 && a[2] == 113))
        }
        IpAddr::V6(ip) => {
            let a = ip.octets();
            !(ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || (a[0] & 0xfe) == 0xfc
                || (a[0] == 0xfe && (a[1] & 0xc0) == 0x80)
                || ip.to_ipv4().is_some_and(|ip| !public_ip(IpAddr::V4(ip))))
        }
    }
}

fn varint(mut n: u32, out: &mut Vec<u8>) {
    while n >= 0x80 {
        out.push((n as u8) | 0x80);
        n >>= 7;
    }
    out.push(n as u8);
}

async fn read_varint(stream: &mut TcpStream) -> anyhow::Result<usize> {
    let mut value = 0usize;
    for shift in (0..35).step_by(7) {
        let byte = stream.read_u8().await?;
        ensure!(shift != 28 || byte <= 0x0f, "varint overflow");
        value |= ((byte & 0x7f) as usize) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    anyhow::bail!("varint too long")
}

fn description(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Object(map) => {
            let mut text = map.get("text").and_then(|v| v.as_str()).unwrap_or_default().to_owned();
            if let Some(extra) = map.get("extra").and_then(|v| v.as_array()) {
                for part in extra {
                    text.push_str(&description(part));
                }
            }
            text
        }
        _ => String::new(),
    }
}

pub fn parse_status(value: &[u8]) -> anyhow::Result<McStatus> {
    ensure!(value.len() <= MAX_PACKET, "status too large");
    let v: serde_json::Value = serde_json::from_slice(value)?;
    Ok(McStatus {
        version: v["version"]["name"]
            .as_str()
            .unwrap_or("Unknown")
            .chars()
            .take(80)
            .collect(),
        online: v["players"]["online"].as_u64().unwrap_or(0).min(u32::MAX as u64) as u32,
        max: v["players"]["max"].as_u64().unwrap_or(0).min(u32::MAX as u64) as u32,
        motd: description(&v["description"]).chars().take(500).collect(),
        favicon: v["favicon"]
            .as_str()
            .filter(|s| s.len() <= 48_000 && s.starts_with("data:image/png;base64,"))
            .map(str::to_owned),
    })
}

async fn modern(mut stream: TcpStream, host: &str, port: u16) -> anyhow::Result<McStatus> {
    let mut body = Vec::new();
    varint(0, &mut body);
    varint(767, &mut body);
    varint(host.len() as u32, &mut body);
    body.extend_from_slice(host.as_bytes());
    body.extend_from_slice(&port.to_be_bytes());
    varint(1, &mut body);
    let mut frame = Vec::new();
    varint(body.len() as u32, &mut frame);
    frame.extend(body);
    frame.extend_from_slice(&[1, 0]);
    stream.write_all(&frame).await?;
    let packet_len = read_varint(&mut stream).await?;
    ensure!(packet_len > 0 && packet_len <= MAX_PACKET, "status packet size invalid");
    let packet_id = read_varint(&mut stream).await?;
    ensure!(packet_id == 0, "unexpected packet");
    let json_len = read_varint(&mut stream).await?;
    ensure!(
        json_len <= MAX_PACKET && json_len <= packet_len,
        "status JSON size invalid"
    );
    let mut json = vec![0; json_len];
    stream.read_exact(&mut json).await?;
    parse_status(&json)
}

fn parse_legacy(bytes: &[u8]) -> anyhow::Result<McStatus> {
    ensure!(bytes.len() >= 3 && bytes[0] == 0xff, "invalid legacy reply");
    let count = u16::from_be_bytes([bytes[1], bytes[2]]) as usize;
    ensure!(
        count <= 4096 && bytes.len() >= 3 + count * 2,
        "legacy reply too large or truncated"
    );
    let text = String::from_utf16(
        &bytes[3..3 + count * 2]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|s| u16::from_be_bytes(*s))
            .collect::<Vec<_>>(),
    )?;
    let parts: Vec<_> = text.split('\0').collect();
    let (version, motd, online, max) = if parts.first() == Some(&"§1") && parts.len() >= 6 {
        (parts[2], parts[3], parts[4], parts[5])
    } else {
        let p: Vec<_> = text.split('§').collect();
        ensure!(p.len() >= 3, "invalid legacy status");
        ("Legacy", p[0], p[1], p[2])
    };
    Ok(McStatus {
        version: version.chars().take(80).collect(),
        online: online.parse()?,
        max: max.parse()?,
        motd: motd.chars().take(500).collect(),
        favicon: None,
    })
}

async fn legacy(mut stream: TcpStream) -> anyhow::Result<McStatus> {
    stream.write_all(&[0xfe, 0x01]).await?;
    let mut header = [0; 3];
    stream.read_exact(&mut header).await?;
    ensure!(header[0] == 0xff, "invalid legacy packet");
    let units = u16::from_be_bytes([header[1], header[2]]) as usize;
    ensure!(units <= 4096, "legacy response too large");
    let mut bytes = header.to_vec();
    bytes.resize(3 + units * 2, 0);
    stream.read_exact(&mut bytes[3..]).await?;
    parse_legacy(&bytes)
}

/// Resolve every address before connecting and reject the whole host if any answer is non-public.
/// Connect to the vetted address, never to the hostname again, to avoid DNS rebinding.
pub async fn ping(input: &str) -> anyhow::Result<McStatus> {
    let (host, port) = parse_host(input)?;
    timeout(DEADLINE, async {
        let addresses: Vec<_> = lookup_host((host.as_str(), port)).await?.collect();
        ensure!(
            !addresses.is_empty() && addresses.iter().all(|a| public_ip(a.ip())),
            "private or reserved address"
        );
        let addr = addresses[0];
        match modern(TcpStream::connect(addr).await?, &host, port).await {
            Ok(status) => Ok(status),
            Err(_) => legacy(TcpStream::connect(addr).await?).await,
        }
    })
    .await
    .context("Minecraft ping timed out")?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_private_and_invalid_hosts() {
        for host in ["localhost:0", "127.0.0.1:25565", "example.com:65536", "a/b", "foo@bar"] {
            if let Ok((name, _)) = parse_host(host) {
                assert!(!public_ip(name.parse().unwrap()));
            }
        }
        assert!(!public_ip("10.1.2.3".parse().unwrap()));
        assert!(!public_ip("::1".parse().unwrap()));
        assert!(!public_ip("::ffff:127.0.0.1".parse().unwrap()));
        assert!(!public_ip("192.0.2.1".parse().unwrap()));
        assert!(parse_host(".").is_err());
        assert!(public_ip("8.8.8.8".parse().unwrap()));
    }
    #[test]
    fn json_description_and_cap() {
        let status = parse_status(br#"{"version":{"name":"1.21"},"players":{"online":2,"max":10},"description":{"text":"Hi","extra":[{"text":"!"}]}}"#).unwrap();
        assert_eq!(status.motd, "Hi!");
        assert!(parse_status(&vec![b'x'; MAX_PACKET + 1]).is_err());
    }
    #[test]
    fn favicon_data_url_is_bounded() {
        assert_eq!(decode_favicon("data:image/png;base64,AQID"), Some(vec![1, 2, 3]));
        assert!(decode_favicon("data:image/jpeg;base64,AQID").is_none());
        assert!(decode_favicon(&format!("data:image/png;base64,{}", "A".repeat(48_004))).is_none());
    }
    #[test]
    fn legacy_fixture() {
        let value = ["§1", "127", "1.21", "Hello", "2", "10"].join("\0");
        let units: Vec<_> = value.encode_utf16().collect();
        let mut bytes = vec![0xff];
        bytes.extend_from_slice(&(units.len() as u16).to_be_bytes());
        for unit in units {
            bytes.extend_from_slice(&unit.to_be_bytes());
        }
        assert_eq!(parse_legacy(&bytes).unwrap().online, 2);
    }
    #[tokio::test]
    async fn legacy_ping_reads_mock_tcp_reply() {
        use tokio::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0; 2];
            stream.read_exact(&mut request).await.unwrap();
            assert_eq!(request, [0xfe, 0x01]);
            let text = ["§1", "127", "1.21", "Legacy MOTD", "4", "12"].join("\0");
            let units: Vec<_> = text.encode_utf16().collect();
            let mut reply = vec![0xff];
            reply.extend_from_slice(&(units.len() as u16).to_be_bytes());
            for unit in units {
                reply.extend_from_slice(&unit.to_be_bytes());
            }
            stream.write_all(&reply).await.unwrap();
        });
        let status = legacy(TcpStream::connect(addr).await.unwrap()).await.unwrap();
        server.await.unwrap();
        assert_eq!(
            (status.online, status.max, status.motd.as_str()),
            (4, 12, "Legacy MOTD")
        );
    }
    #[test]
    fn release_manifest_diff() {
        let old = serde_json::json!({"versions":[{"id":"1.20","type":"release"}]});
        let new = serde_json::json!({"versions":[{"id":"1.21","type":"release"},{"id":"24w01a","type":"snapshot"},{"id":"1.20","type":"release"}]});
        assert_eq!(
            release_ids(&new)
                .into_iter()
                .filter(|id| !release_ids(&old).contains(id))
                .collect::<Vec<_>>(),
            ["1.21"]
        );
    }
    #[tokio::test]
    async fn modern_ping_uses_mock_tcp_and_rejects_overlong_varint() {
        use tokio::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0; 128];
            let n = stream.read(&mut request).await.unwrap();
            assert!(n > 5 && request.windows(2).any(|w| w == b"\x01\x00"));
            let json = br#"{"version":{"name":"1.21"},"players":{"online":3,"max":20},"description":"Hello"}"#;
            let mut payload = vec![0];
            varint(json.len() as u32, &mut payload);
            payload.extend_from_slice(json);
            let mut frame = Vec::new();
            varint(payload.len() as u32, &mut frame);
            frame.extend(payload);
            stream.write_all(&frame).await.unwrap();
        });
        let status = modern(TcpStream::connect(addr).await.unwrap(), "example.com", addr.port())
            .await
            .unwrap();
        server.await.unwrap();
        assert_eq!(status.online, 3);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            stream.write_all(&[0x80, 0x80, 0x80, 0x80, 0x80, 0]).await.unwrap();
        });
        assert!(
            modern(TcpStream::connect(addr).await.unwrap(), "example.com", addr.port())
                .await
                .is_err()
        );
    }
}
