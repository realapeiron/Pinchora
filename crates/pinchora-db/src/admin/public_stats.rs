//! The few totals the public website shows. Plain counts, no player is named.

use crate::Db;

/// How much there is, as the landing page's stats strip tells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublicCounts {
    /// Servers Pinchora keeps settings for (a row appears the first time one is read or changed).
    pub servers: i64,
    /// Players with an account (the first command or the first chat XP makes one).
    pub players: i64,
    /// Houses built.
    pub houses: i64,
    /// Pets adopted and still owned.
    pub pets: i64,
}

impl Db {
    /// The totals for the landing page; slow on a big database, so the web service keeps the answer for a while.
    pub async fn public_counts(&self) -> anyhow::Result<PublicCounts> {
        let (servers, players, houses, pets): (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM guilds), (SELECT count(*) FROM users),
                    (SELECT count(*) FROM houses), (SELECT count(*) FROM pets)",
        )
        .fetch_one(self.pool())
        .await?;
        Ok(PublicCounts {
            servers,
            players,
            houses,
            pets,
        })
    }
}
