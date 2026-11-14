use super::*;

impl App {
    /// The time as game rules that span days and weeks see it (the room contest).
    pub fn now(&self) -> DateTime<Utc> {
        self.services.infrastructure.clock.now()
    }

    /// A gateway epoch must have a fresh invite baseline before joins can be attributed.
    pub fn begin_invite_baseline(&self, guild: u64) -> u64 {
        let mut pending = self.invite_baselines.lock().unwrap();
        let epoch = self.invite_epoch.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        pending.insert(guild, epoch);
        epoch
    }

    pub fn invite_baseline_current(&self, guild: u64, epoch: u64) -> bool {
        self.invite_baselines.lock().unwrap().get(&guild).copied() == Some(epoch)
    }

    pub fn finish_invite_baseline(&self, guild: u64, epoch: u64) {
        let mut pending = self.invite_baselines.lock().unwrap();
        if pending.get(&guild).copied() == Some(epoch) {
            pending.remove(&guild);
        }
    }

    pub fn invite_baseline_pending(&self, guild: u64) -> bool {
        self.invite_baselines.lock().unwrap().contains_key(&guild)
    }

    /// Moves [`App::now`] forward, so tests can end a contest week without waiting for it.
    pub fn shift_clock(&self, by: chrono::Duration) {
        self.services.infrastructure.clock.shift(by);
    }

    pub fn interaction(&self) -> InteractionClient<'_> {
        self.interaction_http.interaction(self.application_id)
    }

    /// Spawns a task that shutdown waits for.
    pub fn spawn<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.services.infrastructure.spawn(future);
    }

    /// Deletes a message of ours after `after`, or as soon as shutdown begins, whichever comes
    /// first: a private answer given in a channel must not outlive the process. The wait is a
    /// tracked task, and a delete that fails is logged.
    pub fn delete_later(&self, channel: Id<ChannelMarker>, message: Id<MessageMarker>, after: Duration) {
        let (http, shutdown) = (self.http.clone(), self.services.infrastructure.cancellation());
        self.spawn(async move {
            tokio::select! {
                () = tokio::time::sleep(after) => {}
                () = shutdown.cancelled() => {}
            }
            if let Err(error) = http.delete_message(channel, message).await {
                tracing::warn!(%error, channel = channel.get(), message = message.get(), "private answer not deleted");
            }
        });
    }

    /// Spawns a gateway event handler, or drops the event when 1024 are already
    /// running. Dropping XP ticks under overload beats unbounded memory.
    pub fn spawn_event<F>(&self, kind: &'static str, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        match self.event_slots.clone().try_acquire_owned() {
            Ok(permit) => {
                self.spawn(async move {
                    future.await;
                    drop(permit);
                });
            }
            Err(_) => metrics::counter!("pinchora_events_dropped_total", "kind" => kind).increment(1),
        }
    }

    /// Spawns a guild rebuild: it waits for one of a few slots of its own (the event slots are for events), so
    /// a storm of GUILD_CREATEs at identify queues up here, as small snapshots, instead of using every slot
    /// the message and member events need.
    pub fn spawn_rebuild<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let slots = self.rebuild_slots.clone();
        self.spawn(async move {
            let Ok(_slot) = slots.acquire_owned().await else {
                return;
            };
            future.await;
        });
    }

    /// Waits up to `limit` for spawned tasks to finish. Tasks that are only waiting out a delay
    /// ([`App::delete_later`]) are told to stop waiting.
    pub async fn drain(&self, limit: Duration) {
        self.services.infrastructure.drain(limit).await;
    }
}
