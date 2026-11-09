use crate::*;
use std::sync::atomic::AtomicBool;
use tokio::net::{TcpListener, TcpStream};

#[tokio::test]
async fn first_room_snapshot_needs_no_new_database_connections() {
    let (Ok(database), Ok(redis)) = (
        std::env::var("PINCHORA_TEST_DATABASE_URL"),
        std::env::var("PINCHORA_TEST_REDIS_URL"),
    ) else {
        return;
    };
    let mut url = reqwest::Url::parse(&database).unwrap();
    let upstream = format!("{}:{}", url.host_str().unwrap(), url.port().unwrap_or(5432));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    url.set_host(Some("127.0.0.1")).unwrap();
    url.set_port(Some(listener.local_addr().unwrap().port())).unwrap();
    let saturated = Arc::new(AtomicBool::new(false));
    let refused = Arc::new(AtomicU64::new(0));
    let (closed, rejected) = (saturated.clone(), refused.clone());
    let proxy = tokio::spawn(async move {
        while let Ok((mut downstream, _)) = listener.accept().await {
            if closed.load(Ordering::SeqCst) {
                rejected.fetch_add(1, Ordering::SeqCst);
                continue;
            }
            let upstream = upstream.clone();
            tokio::spawn(async move {
                let mut upstream = TcpStream::connect(upstream).await.unwrap();
                let _ = tokio::io::copy_bidirectional(&mut downstream, &mut upstream).await;
            });
        }
    });
    let site = Site::connect(url.as_str(), &redis).await;
    let user = unique();
    site.move_in(user, 1, &[]).await;
    let mut browser = site.browser();
    browser.login(user).await;

    // Model a shared server at its connection limit without consuming its slots.
    // Existing sessions keep working, but a lazy pool expansion cannot succeed.
    saturated.store(true, Ordering::SeqCst);
    let response = browser.get("/api/house/rooms/0").await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.text());
    assert_eq!(
        response.json()["fingerprint"],
        pinchora_commands::house::house_edit::fingerprint(&[])
    );
    assert_eq!(
        refused.load(Ordering::SeqCst),
        0,
        "the read reuses its fixture's sessions"
    );
    proxy.abort();
}
