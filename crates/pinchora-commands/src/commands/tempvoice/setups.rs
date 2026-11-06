//! What a server has set up for temp voice, as the voice events see it: its creators and its rooms, kept for
//! half a minute so a voice event costs no query. A server that has nothing set up is known as idle with a
//! single cheap query, and then costs nothing at all.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, LazyLock},
    time::Duration,
};

use moka::future::Cache as Moka;
use pinchora_db::VoiceCreator;

use crate::App;

/// How long a pod trusts what it read. A change made on this pod forgets it at once; one made on another pod
/// (the dashboard) is seen after at most this long.
const KEPT: Duration = Duration::from_secs(30);

#[derive(Debug, Default)]
pub(crate) struct Setup {
    /// Creator channels by id.
    pub creators: HashMap<u64, VoiceCreator>,
    /// The channels of the rooms alive.
    pub rooms: HashSet<u64>,
}

impl Setup {
    pub(crate) fn is_idle(&self) -> bool {
        self.creators.is_empty() && self.rooms.is_empty()
    }
}

static SETUPS: LazyLock<Moka<u64, Arc<Setup>>> =
    LazyLock::new(|| Moka::builder().max_capacity(50_000).time_to_live(KEPT).build());

/// The server's setup.
pub(crate) async fn get(app: &App, guild: u64) -> anyhow::Result<Arc<Setup>> {
    SETUPS
        .try_get_with(guild, async {
            if !app.db.voice_active(guild).await? {
                return anyhow::Ok(Arc::new(Setup::default()));
            }
            let creators = app.db.voice_creators(guild).await?;
            let rooms = app.db.temp_rooms(guild).await?;
            anyhow::Ok(Arc::new(Setup {
                creators: creators.into_iter().map(|c| (c.channel_id as u64, c)).collect(),
                rooms: rooms.into_iter().map(|r| r.channel_id as u64).collect(),
            }))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e:#}"))
}

/// Forgets what this pod read of a server, after a change.
pub(crate) async fn forget(guild: u64) {
    SETUPS.invalidate(&guild).await;
}
