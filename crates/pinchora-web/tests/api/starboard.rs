//! Real nested dashboard routes and their audit trail.
use crate::*;

#[tokio::test]
async fn starboard_nested_deletes_and_all_writes_are_audited() {
    let Some(h) = Harness::new("starboard_nested_deletes_and_all_writes_are_audited").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    let base = format!("/api/guilds/{guild}/starboard");
    let board = json!({"name":"Review","channel_id":GENERAL.to_string(),"emoji":"⭐","threshold":3,
        "self_star":false,"allow_bots":false,"only_channels":[],"except_channels":[],"require_image":false,
        "color":"#ffc857","role_weights":{},"remove_below":true,"enabled":true});
    let saved = browser.write(Method::POST, &format!("{base}/boards"), board).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    let id = saved.json()["id"].as_str().unwrap().to_owned();
    let vote = json!({"channel_id":NEWS.to_string(),"up_emoji":"▲","down_emoji":"▼",
        "only_media":false,"create_thread":true,"enabled":true});
    let saved = browser.write(Method::POST, &format!("{base}/votes"), vote).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    let removed = browser
        .write(Method::DELETE, &format!("{base}/boards/{id}"), json!({}))
        .await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    let removed = browser
        .write(Method::DELETE, &format!("{base}/votes/{NEWS}"), json!({}))
        .await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    let rows = h.state.app.db.audit_page(guild, None, 10).await.unwrap();
    let actions: Vec<_> = rows.iter().map(|row| row.action.as_str()).collect();
    for action in [
        "starboard.save",
        "starboard.delete",
        "starboard.vote.save",
        "starboard.vote.delete",
    ] {
        assert!(actions.contains(&action), "missing {action}: {actions:?}");
    }
    assert!(
        rows.iter()
            .all(|row| row.actor_id == owner as i64 && row.before != row.after)
    );
}
