use crate::Sim;

/// Puts a cast of `user` in the cache as if it had bitten `late_ms` ago. `state` is the part of the
/// value after the time (`spot:rod:bait`), or empty for a cast that carries none.
pub(super) async fn seed_cast(sim: &Sim, user: u64, nonce: u32, late_ms: i64, state: &str) -> String {
    let at = sim.app.now().timestamp_millis() - late_ms;
    let value = if state.is_empty() {
        at.to_string()
    } else {
        format!("{at}:{state}")
    };
    sim.app
        .cache
        .put(
            &format!("cast:{user}:{nonce}"),
            &value,
            std::time::Duration::from_secs(30),
        )
        .await
        .unwrap();
    format!("fr:{user}:{nonce}")
}

/// Keep rod boundaries exact across cache, database and dispatch awaits.
pub(super) async fn seed_fixed_cast(sim: &Sim, user: u64, nonce: u32, late_ms: i64, state: &str) -> String {
    sim.app.services.infrastructure.clock.freeze(sim.app.now());
    seed_cast(sim, user, nonce, late_ms, state).await
}
