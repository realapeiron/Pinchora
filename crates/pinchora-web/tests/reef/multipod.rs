use super::*;

async fn eligible(site: &Site, user: u64) -> Browser {
    let browser = site.browser(user).await;
    site.app
        .db
        .reef_consent(user, pinchora_db::REEF_CONSENT_VERSION, "13_plus", true)
        .await
        .unwrap();
    browser
}
async fn assert_no_layout(player: &mut Player) {
    let _ = tokio::time::timeout(Duration::from_millis(600), async {
        while let Some(Ok(message)) = player.socket.next().await {
            if let Message::Text(text) = message {
                let body: Value = serde_json::from_str(&text).unwrap();
                assert_ne!(body["type"], "layout_changed", "duplicate layout notification");
            }
        }
    })
    .await;
}

#[tokio::test]
async fn two_pods_presence_movement_chat_and_transactional_outbox() {
    let Some(a) = Site::new().await else {
        return;
    };
    let b = Site::new().await.unwrap();
    let host = unique();
    let guest = unique();
    let guest_id = guest.to_string();
    a.house(host).await;
    let alice = eligible(&a, host).await;
    let bob = eligible(&b, guest).await;
    let mut p = alice.connect().await;
    let mut q = bob.connect().await;
    assert_eq!(p.join(host).await["players"].as_array().unwrap().len(), 1);
    let snapshot = q.join(host).await;
    assert_eq!(snapshot["players"].as_array().unwrap().len(), 2);
    assert_eq!(p.epoch, q.epoch);
    loop {
        if p.until("presence").await["player"]["user_id"] == guest_id {
            break;
        }
    }
    p.send(json!({"type":"move_to","tile":[2,7]})).await;
    assert_eq!(p.until("path").await["path"], q.until("path").await["path"]);
    q.send(json!({"type":"chat","text":"Hello from another pod"})).await;
    let sent = q.until("chat").await;
    let delivered = p.until("chat").await;
    assert_eq!(sent["message_id"], delivered["message_id"]);
    assert_eq!(sent["text"], "Hello from another pod");
    let token = b
        .app
        .cache
        .reef_get(&format!("controller:{guest}"))
        .await
        .unwrap()
        .unwrap();
    b.app
        .cache
        .reef_publish(
            &format!("house:{host}:0"),
            &json!({"route":"Join","token":token,"user":guest,
        "name":"guest","character":Character::starter(Content::get()).encode(),"request":"retry-join"})
            .to_string(),
        )
        .await
        .unwrap();
    q.until("snapshot").await;
    b.app
        .cache
        .reef_publish(
            &format!("house:{host}:0"),
            &json!({"route":"Command","token":token,"envelope":{
                "protocol_version":1,"request_id":q.sequence.to_string(),"room_epoch":q.epoch,"sequence":q.sequence,
                "type":"chat","text":"Hello from another pod"
            }})
            .to_string(),
        )
        .await
        .unwrap();
    q.until("snapshot").await;
    let evidence: i64 = sqlx::query_scalar("SELECT count(*) FROM reef_chat_evidence WHERE sender_id=$1")
        .bind(guest as i64)
        .fetch_one(&a.pool)
        .await
        .unwrap();
    assert_eq!(evidence, 1, "duplicate relay commands do not repeat durable effects");

    let before = a.app.db.reef_layout_events(host).await.unwrap();
    let mut tx = a.pool.begin().await.unwrap();
    sqlx::query("UPDATE rooms SET wallpaper='rollback-only' WHERE user_id=$1")
        .bind(host as i64)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(before, a.app.db.reef_layout_events(host).await.unwrap());
    // Bot and web both use these tables. Two writes in one transaction produce one outbox event.
    let content = Content::get();
    let mut tx = a.pool.begin().await.unwrap();
    sqlx::query("UPDATE rooms SET wallpaper=$2 WHERE user_id=$1")
        .bind(host as i64)
        .bind(&content.surfaces.wallpapers[1].id)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE rooms SET floor=$2 WHERE user_id=$1")
        .bind(host as i64)
        .bind(&content.surfaces.floors[1].id)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_eq!(
        before,
        a.app.db.reef_layout_events(host).await.unwrap(),
        "uncommitted outbox is invisible"
    );
    tx.commit().await.unwrap();
    let after = a.app.db.reef_layout_events(host).await.unwrap();
    assert_eq!(after.len(), before.len() + 1);
    let first = p.until("layout_changed").await;
    let second = q.until("layout_changed").await;
    assert_eq!(first["revision"], second["revision"]);
    assert_eq!(first["event_ids"], json!([after.last().unwrap().to_string()]));
    p.until("snapshot").await;
    q.until("snapshot").await;
    for _ in 0..3 {
        a.app
            .cache
            .reef_publish(
                &format!("house:{host}:0"),
                &json!({"route":"Layout","id":after.last().unwrap()}).to_string(),
            )
            .await
            .unwrap();
    }
    tokio::join!(assert_no_layout(&mut p), assert_no_layout(&mut q));
    q.send(json!({"type":"leave"})).await;
    loop {
        if p.until("presence").await["action"] == "leave" {
            break;
        }
    }
}

#[tokio::test]
async fn controller_replacement_and_safety_reach_another_pod() {
    let Some(a) = Site::new().await else {
        return;
    };
    let b = Site::new().await.unwrap();
    let user = unique();
    a.house(user).await;
    let alice = eligible(&a, user).await;
    let bob = eligible(&b, user).await;
    let mut old = alice.connect().await;
    old.join(user).await;
    let mut new = bob.connect().await;
    assert_eq!(old.close_reason().await, "controller_replaced");
    assert_eq!(new.join(user).await["players"].as_array().unwrap().len(), 1);
    // The HTTP request hits A while the active socket is on B.
    assert_eq!(
        alice
            .api(
                reqwest::Method::PUT,
                "/api/reef/consent",
                json!({"version":pinchora_db::REEF_CONSENT_VERSION,"age_band":"13_plus","accepted":false})
            )
            .await
            .status(),
        200
    );
    assert_eq!(new.close_reason().await, "consent_revoked");
}

#[tokio::test]
async fn owner_pod_kill_expires_lease_and_reconnects_with_new_fence() {
    let Some(a) = Site::new().await else {
        return;
    };
    let b = Site::new().await.unwrap();
    let host = unique();
    a.house(host).await;
    let mut p = eligible(&a, host).await.connect().await;
    let guest = eligible(&b, unique()).await;
    let mut q = guest.connect().await;
    p.join(host).await;
    q.join(host).await;
    let epoch: i64 = q.epoch.parse().unwrap();
    let fence = pinchora_db::ReefFence {
        namespace: a.app.cache.namespace().into(),
        owner: host,
        idx: 0,
        token: epoch,
    };
    // Stop every actor/relay task without releasing room leases, as with a killed process.
    a.server.abort();
    a.state.drain_reef();
    // Commit while the owner is gone: there is no publisher left to acknowledge this edit.
    let floor = &Content::get().surfaces.floors[1].id;
    sqlx::query("UPDATE rooms SET floor=$2 WHERE user_id=$1 AND idx=0")
        .bind(host as i64)
        .bind(floor)
        .execute(&b.pool)
        .await
        .unwrap();
    assert_eq!(q.close_reason().await, "ownership_lost");
    let mut recovered = guest.connect().await;
    let snapshot = recovered.join(host).await;
    assert!(recovered.epoch.parse::<i64>().unwrap() > epoch);
    assert_eq!(snapshot["players"].as_array().unwrap().len(), 1);
    assert_eq!(snapshot["layout"]["floor"], *floor);
    assert!(!b.app.db.reef_renew(&fence).await.unwrap());
    assert!(
        !b.app
            .cache
            .reef_publish_fenced(
                &format!("lease:house:{host}:0"),
                &epoch.to_string(),
                &format!("house:{host}:0"),
                "stale"
            )
            .await
            .unwrap()
    );
    let rejected = b
        .app
        .db
        .reef_chat_fenced(host, 0, host, "stale owner", &[host], Some(&fence))
        .await
        .unwrap();
    assert!(matches!(rejected, Err("ownership_lost")));
    recovered.send(json!({"type":"move_to","tile":[1,7]})).await;
    recovered.until("path").await;
}

#[tokio::test]
async fn environment_isolation_includes_legacy_invalidation() {
    let Ok(url) = std::env::var("PINCHORA_TEST_REDIS_URL") else {
        return;
    };
    let tag = unique();
    let a = Cache::connect_scoped(&url, &format!("reef_a_{tag}")).await.unwrap();
    let b = Cache::connect_scoped(&url, &format!("reef_b_{tag}")).await.unwrap();
    assert_ne!(a.invalidation_channel(), "pinchora:invalidate");
    assert!(a.invalidation_channel().contains(":db10:"));
    let mut live = Box::pin(b.invalidations().await.unwrap());
    let mut own = Box::pin(a.invalidations().await.unwrap());
    a.publish_invalidation(Invalidation::House(tag)).await.unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), own.next()).await.unwrap(),
        Some(Invalidation::House(tag))
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(100), live.next())
            .await
            .is_err()
    );
    a.reef_claim("controller:123", "test", 1000, true).await.unwrap();
    assert_eq!(b.reef_get("controller:123").await.unwrap(), None);
    let mut other = Box::pin(b.reef_subscribe("house:123:0").await.unwrap());
    a.reef_publish("house:123:0", "test").await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(100), other.next())
            .await
            .is_err()
    );
}

/// Dedicated child Redis, never a shared Redis restart or a container operation.
struct RedisProcess {
    child: std::process::Child,
    port: u16,
}
impl RedisProcess {
    async fn start(port: u16) -> Self {
        let child = std::process::Command::new("redis-server")
            .args([
                "--bind",
                "127.0.0.1",
                "--port",
                &port.to_string(),
                "--save",
                "",
                "--appendonly",
                "no",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("redis-server installed for outage test");
        let process = Self { child, port };
        for _ in 0..100 {
            if TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
                return process;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("private Redis did not start");
    }
    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
    fn url(&self) -> String {
        format!("redis://127.0.0.1:{}/10", self.port)
    }
}
impl Drop for RedisProcess {
    fn drop(&mut self) {
        self.stop();
    }
}

#[tokio::test]
async fn redis_restart_fails_closed_then_snapshot_recovers_on_two_pods() {
    if std::env::var("PINCHORA_TEST_DATABASE_URL").is_err() {
        return;
    }
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let mut redis = RedisProcess::start(port).await;
    let a = Site::with_redis(Some(&redis.url())).await.unwrap();
    let b = Site::with_redis(Some(&redis.url())).await.unwrap();
    let host = unique();
    a.house(host).await;
    let mut p = eligible(&a, host).await.connect().await;
    let mut q = eligible(&b, unique()).await.connect().await;
    p.join(host).await;
    q.join(host).await;
    let epoch = q.epoch.parse::<i64>().unwrap();
    redis.stop();
    // Recreate an empty Redis: a numerically older fence must never be accepted after key loss.
    drop(redis);
    let _restarted = RedisProcess::start(port).await;
    for player in [&mut p, &mut q] {
        let reason = player.close_reason().await;
        assert!(
            ["routing_unavailable", "ownership_lost", "session_expired"].contains(&reason.as_str()),
            "{reason}"
        );
    }
    tokio::time::sleep(Duration::from_secs(3)).await;
    let mut p = eligible(&a, host).await.connect().await;
    let mut q = eligible(&b, unique()).await.connect().await;
    p.join(host).await;
    assert!(p.epoch.parse::<i64>().unwrap() > epoch);
    assert_eq!(q.join(host).await["players"].as_array().unwrap().len(), 2);
    p.send(json!({"type":"chat","text":"Redis is back"})).await;
    assert_eq!(p.until("chat").await["message_id"], q.until("chat").await["message_id"]);
}

#[tokio::test]
async fn outbox_delivers_a_lower_id_that_commits_after_a_higher_id() {
    let Some(a) = Site::new().await else {
        return;
    };
    let b = Site::new().await.unwrap();
    let host = unique();
    a.house(host).await;
    sqlx::query("INSERT INTO rooms(user_id,idx,room_type,wallpaper,floor) SELECT user_id,1,room_type,wallpaper,floor FROM rooms WHERE user_id=$1 AND idx=0")
        .bind(host as i64).execute(&a.pool).await.unwrap();
    let mut p = eligible(&a, host).await.connect().await;
    let mut q = eligible(&b, unique()).await.connect().await;
    p.join(host).await;
    q.join(host).await;
    let mut late = a.pool.begin().await.unwrap();
    sqlx::query("UPDATE rooms SET wallpaper=$2 WHERE user_id=$1 AND idx=0")
        .bind(host as i64)
        .bind(&Content::get().surfaces.wallpapers[1].id)
        .execute(&mut *late)
        .await
        .unwrap();
    let low: i64 =
        sqlx::query_scalar("SELECT id FROM reef_layout_outbox WHERE owner_id=$1 AND transaction_id=txid_current()")
            .bind(host as i64)
            .fetch_one(&mut *late)
            .await
            .unwrap();
    // A separate room allows the later transaction to commit without waiting for the first row lock.
    sqlx::query("UPDATE rooms SET floor=$2 WHERE user_id=$1 AND idx=1")
        .bind(host as i64)
        .bind(&Content::get().surfaces.floors[1].id)
        .execute(&a.pool)
        .await
        .unwrap();
    let high = p.until("layout_changed").await["event_id"]
        .as_str()
        .unwrap()
        .parse::<i64>()
        .unwrap();
    assert!(high > low);
    assert_eq!(q.until("layout_changed").await["event_id"], high.to_string());
    p.until("snapshot").await;
    q.until("snapshot").await;
    late.commit().await.unwrap();
    assert_eq!(p.until("layout_changed").await["event_id"], low.to_string());
    assert_eq!(q.until("layout_changed").await["event_id"], low.to_string());
    tokio::join!(assert_no_layout(&mut p), assert_no_layout(&mut q));
}

#[tokio::test]
async fn simultaneous_pod_admission_elects_one_room_owner() {
    let Some(a) = Site::new().await else {
        return;
    };
    let b = Site::new().await.unwrap();
    let host = unique();
    a.house(host).await;
    let mut p = eligible(&a, host).await.connect().await;
    let mut q = eligible(&b, unique()).await.connect().await;
    let (left, right) = tokio::join!(p.join(host), q.join(host));
    assert_eq!(left["room_epoch"], right["room_epoch"]);
    p.send(json!({"type":"snapshot"})).await;
    q.send(json!({"type":"snapshot"})).await;
    assert_eq!(p.until("snapshot").await["players"].as_array().unwrap().len(), 2);
    assert_eq!(q.until("snapshot").await["players"].as_array().unwrap().len(), 2);
    let leases: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM reef_room_leases WHERE namespace=$1 AND owner_id=$2 AND expires_at>clock_timestamp()",
    )
    .bind(a.app.cache.namespace())
    .bind(host as i64)
    .fetch_one(&a.pool)
    .await
    .unwrap();
    assert_eq!(leases, 1);
}

#[tokio::test]
async fn a_lost_leave_does_not_keep_presence_in_the_previous_room() {
    let Some(a) = Site::new().await else {
        return;
    };
    let b = Site::new().await.unwrap();
    let host = unique();
    let guest = unique();
    let guest_id = guest.to_string();
    a.house(host).await;
    let mut observer = eligible(&a, host).await.connect().await;
    observer.join(host).await;
    let _browser = eligible(&b, guest).await;
    let token = format!("lost-leave-{guest}");
    b.app
        .cache
        .reef_claim(&format!("controller:{guest}"), &token, 15000, true)
        .await
        .unwrap();
    b.app
        .cache
        .reef_claim(&format!("location:{token}"), &format!("house:{host}:0"), 15000, true)
        .await
        .unwrap();
    b.app
        .cache
        .reef_publish(
            &format!("house:{host}:0"),
            &json!({"route":"Join","token":token,"user":guest,"name":"guest",
        "character":Character::starter(Content::get()).encode(),"request":"join"})
            .to_string(),
        )
        .await
        .unwrap();
    loop {
        if observer.until("presence").await["player"]["user_id"] == guest_id {
            break;
        }
    }
    // Simulate a transition whose Leave publication was lost. Its controller remains alive.
    b.app
        .cache
        .reef_claim(&format!("location:{token}"), &format!("house:{host}:1"), 15000, true)
        .await
        .unwrap();
    let left = observer.until("presence").await;
    assert_eq!(left["action"], "leave");
    assert_eq!(left["user_id"], guest_id);
    assert_eq!(
        b.app.cache.reef_get(&format!("controller:{guest}")).await.unwrap(),
        Some(token.clone())
    );
    b.app
        .cache
        .reef_release(&format!("controller:{guest}"), &token)
        .await
        .unwrap();
    b.app
        .cache
        .reef_release(&format!("location:{token}"), &format!("house:{host}:1"))
        .await
        .unwrap();
}

#[tokio::test]
async fn expired_fences_cannot_be_renewed_or_write_after_waiting_for_a_row_lock() {
    let Some(site) = Site::new().await else { return };
    let host = unique();
    site.house(host).await;
    let _browser = eligible(&site, host).await;
    let fence = site
        .app
        .db
        .reef_acquire(site.app.cache.namespace(), host, 0)
        .await
        .unwrap()
        .unwrap();
    let mut blocker = site.pool.begin().await.unwrap();
    sqlx::query("SELECT token FROM reef_room_leases WHERE owner_id=$1 FOR UPDATE")
        .bind(host as i64)
        .execute(&mut *blocker)
        .await
        .unwrap();
    let db = site.app.db.clone();
    let old = fence.clone();
    let renewal = tokio::spawn(async move { db.reef_renew(&old).await.unwrap() });
    tokio::time::sleep(Duration::from_millis(3300)).await;
    blocker.commit().await.unwrap();
    assert!(
        !renewal.await.unwrap(),
        "a lock wait must not resurrect an expired lease"
    );
    let fresh = site
        .app
        .db
        .reef_acquire(site.app.cache.namespace(), host, 0)
        .await
        .unwrap()
        .unwrap();
    assert!(fresh.token > fence.token);
    let mut blocker = site.pool.begin().await.unwrap();
    sqlx::query("SELECT token FROM reef_room_leases WHERE owner_id=$1 FOR UPDATE")
        .bind(host as i64)
        .execute(&mut *blocker)
        .await
        .unwrap();
    let db = site.app.db.clone();
    let write = tokio::spawn(async move {
        db.reef_chat_fenced(host, 0, host, "expired during wait", &[host], Some(&fresh))
            .await
            .unwrap()
    });
    tokio::time::sleep(Duration::from_millis(3300)).await;
    blocker.commit().await.unwrap();
    assert!(matches!(write.await.unwrap(), Err("ownership_lost")));
}

#[tokio::test]
async fn chat_recipient_authorization_is_rechecked_against_remote_moderation() {
    let Some(a) = Site::new().await else { return };
    let b = Site::new().await.unwrap();
    let host = unique();
    let sender = unique();
    let banned = unique();
    let blocked = unique();
    a.house(host).await;
    for user in [host, sender, banned, blocked] {
        let _ = eligible(&a, user).await;
    }
    let moderator = b.browser(host).await;
    assert_eq!(
        moderator
            .api(
                reqwest::Method::POST,
                "/api/reef/moderation",
                json!({"target":banned.to_string(),"owner":host.to_string(),"room_idx":0,"kind":"ban","seconds":60})
            )
            .await
            .status(),
        204
    );
    assert_eq!(
        moderator
            .api(reqwest::Method::PUT, &format!("/api/reef/blocks/{blocked}"), json!({}))
            .await
            .status(),
        204
    );
    // The owner's roster can still contain remote users until the next policy sweep.
    let (evidence, recipients) = a
        .app
        .db
        .reef_chat(
            host,
            0,
            sender,
            "private room message",
            &[host, sender, banned, blocked],
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(recipients, vec![host, sender]);
    for user in [banned, blocked] {
        assert!(
            a.app
                .db
                .reef_report(user, evidence.id, "other")
                .await
                .unwrap()
                .is_none()
        );
    }
    a.app.db.set_house_locked(host, true).await.unwrap();
    let (_, recipients) = a
        .app
        .db
        .reef_chat(host, 0, host, "locked room message", &[host, sender])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(recipients, vec![host]);
}

#[tokio::test]
async fn room_lease_renews_while_chat_waits_for_player_locks() {
    let Some(a) = Site::new().await else { return };
    let host = unique();
    a.house(host).await;
    let mut p = eligible(&a, host).await.connect().await;
    p.join(host).await;
    let epoch = p.epoch.clone();
    let mut blocker = a.pool.begin().await.unwrap();
    sqlx::query("SELECT lock_user($1)")
        .bind(host as i64)
        .execute(&mut *blocker)
        .await
        .unwrap();
    p.send(json!({"type":"chat","text":"waiting for the player lock"}))
        .await;
    tokio::time::sleep(Duration::from_millis(3200)).await;
    assert_eq!(
        a.app
            .cache
            .reef_get(&format!("lease:house:{host}:0"))
            .await
            .unwrap()
            .as_deref(),
        Some(epoch.as_str())
    );
    let alive: bool = sqlx::query_scalar("SELECT expires_at>clock_timestamp() FROM reef_room_leases WHERE owner_id=$1")
        .bind(host as i64)
        .fetch_one(&a.pool)
        .await
        .unwrap();
    assert!(alive, "busy actors must renew independently of the command queue");
    blocker.commit().await.unwrap();
}

#[tokio::test]
async fn remote_mute_and_block_cover_owner_commands_and_delayed_chat_delivery() {
    let Some(a) = Site::new().await else { return };
    let b = Site::new().await.unwrap();
    let host = unique();
    let guest = unique();
    let observer = unique();
    a.house(host).await;
    let mut p = eligible(&a, host).await.connect().await;
    p.join(host).await;
    let mut q = eligible(&b, guest).await.connect().await;
    q.join(host).await;
    let mut r = eligible(&b, observer).await.connect().await;
    r.join(host).await;
    let remote_host = b.browser(host).await;
    assert_eq!(
        remote_host
            .api(
                reqwest::Method::POST,
                "/api/reef/moderation",
                json!({"target":guest.to_string(),"owner":host.to_string(),"room_idx":0,"kind":"mute","seconds":60})
            )
            .await
            .status(),
        204
    );
    q.send(json!({"type":"chat","text":"muted on another pod"})).await;
    assert_eq!(q.until("error").await["reason"], "chat_muted");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM reef_chat_evidence WHERE sender_id=$1")
        .bind(guest as i64)
        .fetch_one(&a.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    // Evidence was authorized before a block, but its relay arrives after the block committed.
    let (message, _) = a
        .app
        .db
        .reef_chat(host, 0, observer, "queued private chat", &[host, observer, guest])
        .await
        .unwrap()
        .unwrap();
    assert!(a.app.db.reef_chat_deliverable(message.id, guest).await.unwrap());
    let remote_guest = b.browser(guest).await;
    assert_eq!(
        remote_guest
            .api(reqwest::Method::PUT, &format!("/api/reef/blocks/{observer}"), json!({}))
            .await
            .status(),
        204
    );
    q.until("policy_changed").await;
    assert!(!a.app.db.reef_chat_deliverable(message.id, guest).await.unwrap());
    let token = b
        .app
        .cache
        .reef_get(&format!("controller:{guest}"))
        .await
        .unwrap()
        .unwrap();
    let mut body = message.json();
    body["type"] = json!("chat");
    a.app
        .cache
        .reef_publish(
            &format!("house:{host}:0"),
            &json!({"route":"Event","token":token,
        "event":{"delivery":1,"stream":"delayed-private-chat","epoch":p.epoch,"request":"","body":body}})
            .to_string(),
        )
        .await
        .unwrap();
    let _ = tokio::time::timeout(Duration::from_millis(600), async {
        while let Some(Ok(Message::Text(text))) = q.socket.next().await {
            let body: Value = serde_json::from_str(&text).unwrap();
            assert_ne!(body["type"], "chat", "a delayed event bypassed a committed block");
        }
    })
    .await;
    assert_eq!(
        remote_host
            .api(
                reqwest::Method::POST,
                "/api/reef/moderation",
                json!({"target":guest.to_string(),"owner":host.to_string(),"room_idx":0,"kind":"ban","seconds":60})
            )
            .await
            .status(),
        204
    );
    assert_eq!(q.close_reason().await, "sanctioned");
}

#[tokio::test]
async fn recovery_snapshots_do_not_bypass_remote_access_revocation() {
    let Some(a) = Site::new().await else { return };
    let b = Site::new().await.unwrap();
    let host = unique();
    a.house(host).await;
    let mut p = eligible(&a, host).await.connect().await;
    p.join(host).await;
    let mut q = eligible(&b, unique()).await.connect().await;
    q.join(host).await;
    b.app.db.set_house_locked(host, true).await.unwrap();
    q.send(json!({"type":"snapshot"})).await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(Ok(message)) = q.socket.next().await {
            match message {
                Message::Text(text) => {
                    let body: Value = serde_json::from_str(&text).unwrap();
                    assert_ne!(body["type"], "snapshot", "recovery disclosed a revoked private room");
                }
                Message::Close(Some(frame)) => {
                    assert_eq!(frame.reason, "access_revoked");
                    return;
                }
                _ => {}
            }
        }
        panic!("expected an access-revoked close");
    })
    .await
    .unwrap();
}
