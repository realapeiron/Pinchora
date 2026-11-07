//! Telling every pod that cached settings changed, so a dashboard edit reaches the bot at once
//! instead of after the cache's minute.

use futures_util::{Stream, StreamExt};
use redis::AsyncCommands;

use crate::Cache;

/// The pub/sub channel changes are announced on. Pub/sub ignores the database number, so every
/// Pinchora on one Redis hears it; the ids in the messages keep that harmless.
pub const INVALIDATE_CHANNEL: &str = "pinchora:invalidate";

/// What changed. The message on the wire is `guild:<id>` or `xp:<id>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invalidation {
    /// A server's settings row.
    Guild(u64),
    /// A server's XP multipliers.
    XpRules(u64),
    /// Command IDs/overrides changed for this application.
    Commands(u64),
}

impl Invalidation {
    pub fn encode(self) -> String {
        match self {
            Self::Guild(id) => format!("guild:{id}"),
            Self::XpRules(id) => format!("xp:{id}"),
            Self::Commands(id) => format!("commands:{id}"),
        }
    }

    pub fn decode(message: &str) -> Option<Self> {
        let (kind, id) = message.split_once(':')?;
        let id = id.parse().ok()?;
        match kind {
            "guild" => Some(Self::Guild(id)),
            "xp" => Some(Self::XpRules(id)),
            "commands" => Some(Self::Commands(id)),
            _ => None,
        }
    }
}

impl Cache {
    /// Announces a change; returns how many subscribers heard it.
    pub async fn publish_invalidation(&self, what: Invalidation) -> anyhow::Result<usize> {
        Ok(self.conn().publish(INVALIDATE_CHANNEL, what.encode()).await?)
    }

    /// Subscribes to announced changes. Returns once subscribed; the stream ends when the
    /// connection drops, and messages it cannot read are skipped.
    pub async fn invalidations(&self) -> anyhow::Result<impl Stream<Item = Invalidation> + Send + use<>> {
        let mut pubsub = self.client.get_async_pubsub().await?;
        pubsub.subscribe(INVALIDATE_CHANNEL).await?;
        Ok(pubsub.into_on_message().filter_map(|message| async move {
            let payload: String = message.get_payload().ok()?;
            Invalidation::decode(&payload)
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip() {
        for what in [
            Invalidation::Guild(42),
            Invalidation::XpRules(u64::MAX >> 1),
            Invalidation::Commands(123),
        ] {
            assert_eq!(Invalidation::decode(&what.encode()), Some(what));
        }
        assert_eq!(Invalidation::Guild(7).encode(), "guild:7");
        for junk in ["", "guild", "guild:", "guild:x", "pets:1", ":1"] {
            assert_eq!(Invalidation::decode(junk), None, "{junk}");
        }
    }
}
