//! End-to-end member profile-code flow through mock Roblox and Discord HTTP.
use super::*;

#[tokio::test]
async fn roblox_profile_code_english_and_turkish() {
    let Some(mut sim) = Sim::new("roblox_profile_code_english_and_turkish").await else {
        return;
    };
    let description = Arc::new(Mutex::new(String::new()));
    let profile = description.clone();
    let roblox_id = unique();
    let app = Router::new()
        .route(
            "/v1/usernames/users",
            axum::routing::post(
                move || async move { axum::Json(json!({"data":[{"id":roblox_id,"name":"CrabPlayer"}]})) },
            ),
        )
        .route(
            "/v1/users/{id}",
            axum::routing::get(move || {
                let value = profile.lock().unwrap().clone();
                async move { axum::Json(json!({"description":value})) }
            }),
        );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    pinchora_commands::roblox::install_test_endpoint(Some(&endpoint));
    let user = unique();
    let start = sim
        .command(
            user,
            "roblox",
            json!([sub("verify", vec![text_option("username", "CrabPlayer")])]),
        )
        .await;
    assert!(
        start.ephemeral && start.content().contains("About"),
        "{}",
        start.content()
    );
    let pending = sim.app.db.roblox_pending(user).await.unwrap().unwrap();
    *description.lock().unwrap() = format!("my beach profile {}", pending.code);
    let verified = sim.click(user, &start, "Check profile").await;
    assert!(
        verified.ephemeral && verified.content().contains("linked"),
        "{}",
        verified.content()
    );
    assert_eq!(
        sim.app.db.roblox_link(user).await.unwrap().unwrap().roblox_id,
        roblox_id as i64
    );
    let whois = sim.command(user, "roblox", json!([sub("whois", vec![])])).await;
    assert!(whois.ephemeral);
    sim.locale = "tr";
    let gone = sim.command(user, "roblox", json!([sub("unverify", vec![])])).await;
    assert!(
        gone.ephemeral && gone.content().contains("kaldırıldı"),
        "{}",
        gone.content()
    );
    assert!(sim.app.db.roblox_link(user).await.unwrap().is_none());
    pinchora_commands::roblox::install_test_endpoint(None);
}
