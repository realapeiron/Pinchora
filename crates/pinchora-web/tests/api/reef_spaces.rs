use crate::*;

#[tokio::test]
async fn guild_plaza_membership_delegated_management_capacity_and_scheduled_rsvp() {
    let Some(h) = Harness::new("reef7_plazas").await else {
        return;
    };
    let (guild, owner, mut manager) = h.managed_server().await;
    h.state.app.db.guild_settings(guild).await.unwrap();
    let content = pinchora_game::Content::get();
    h.state
        .app
        .db
        .house(
            guild,
            &pinchora_db::NewRoom {
                room_type: "living".into(),
                wallpaper: content.surfaces.wallpapers[0].id.clone(),
                floor: content.surfaces.floors[0].id.clone(),
            },
        )
        .await
        .unwrap();
    let user = unique();
    let mut member = h.browser();
    h.mock.member_roles.lock().unwrap().insert(user, vec![FAN]);
    member
        .login(&h, user, vec![guild_entry(guild, "Plaza", false, 0)])
        .await;
    let path = format!("/api/reef/guilds/{guild}/plazas");
    let admitted = member.get(&path).await;
    assert_eq!(admitted.status, StatusCode::OK, "{}", admitted.text());
    assert_eq!(admitted.json()["room"], format!("plaza:{guild}:0:0"));
    h.state
        .app
        .cache
        .reef_claim(&format!("occupancy:plaza:{guild}:0:0"), "32", 3000, true)
        .await
        .unwrap();
    assert_eq!(member.get(&path).await.json()["room"], format!("plaza:{guild}:0:1"));
    assert_eq!(
        member.get(&format!("{path}?instance=0")).await.status,
        StatusCode::CONFLICT
    );
    let edit = format!("/api/house/rooms/0/ops?scope=plaza&guild={guild}");
    let room = member
        .get(&format!("/api/house/rooms/0?scope=plaza&guild={guild}"))
        .await;
    let body = json!({"expected":room.json()["fingerprint"],"op":{"type":"clear"}});
    assert_eq!(
        member.write(Method::POST, &edit, body.clone()).await.status,
        StatusCode::FORBIDDEN
    );
    for op in [
        json!({"type":"place","item_id":"pebblerest_stool__sand","x":1,"y":1,"flipped":false}),
        json!({"type":"move","id":"1","x":1,"y":1,"flipped":false}),
        json!({"type":"flip","id":"1"}),
        json!({"type":"pickup","id":"1"}),
        json!({"type":"undo"}),
        json!({"type":"surface","wallpaper":"paper"}),
    ] {
        assert_eq!(
            member
                .write(
                    Method::POST,
                    &edit,
                    json!({"expected":room.json()["fingerprint"],"op":op})
                )
                .await
                .status,
            StatusCode::FORBIDDEN
        );
    }
    h.state
        .app
        .db
        .replace_dashboard_managers(
            guild,
            owner,
            "Owner",
            &[pinchora_db::DashboardManager {
                role_id: FAN as i64,
                areas: vec!["configuration".into(), "safety".into()],
            }],
        )
        .await
        .unwrap();
    assert_eq!(
        member.write(Method::POST, &edit, body.clone()).await.status,
        StatusCode::FORBIDDEN
    );
    h.state
        .app
        .db
        .replace_dashboard_managers(
            guild,
            owner,
            "Owner",
            &[pinchora_db::DashboardManager {
                role_id: FAN as i64,
                areas: vec!["community".into()],
            }],
        )
        .await
        .unwrap();
    assert_eq!(
        member.write(Method::POST, &edit, body.clone()).await.status,
        StatusCode::OK
    );
    // Community delegates can moderate this plaza, never another host or a global scope.
    assert_eq!(
        member.get(&format!("/api/reef/moderation?guild={guild}")).await.status,
        StatusCode::OK
    );
    let action = json!({"target":unique().to_string(),"owner":guild.to_string(),"room_idx":null,"kind":"mute","seconds":60,"report":null});
    assert_eq!(
        member
            .write(Method::POST, "/api/reef/moderation", action.clone())
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    let mut global = action;
    global["owner"] = Value::Null;
    assert_eq!(
        member.write(Method::POST, "/api/reef/moderation", global).await.status,
        StatusCode::FORBIDDEN
    );
    let gatherings = format!("/api/reef/guilds/{guild}/gatherings");
    let schedule = json!({"title":"Reef room tour","starts_at":chrono::Utc::now()+chrono::Duration::hours(1),"ends_at":chrono::Utc::now()+chrono::Duration::hours(2)});
    let event = member.write(Method::POST, &gatherings, schedule.clone()).await;
    assert_eq!(event.status, StatusCode::OK, "{}", event.text());
    let event_id = event.json()["id"].as_str().unwrap().to_owned();
    let rsvp = member
        .write(
            Method::POST,
            &format!("{gatherings}/{event_id}/rsvp"),
            json!({"option":"going"}),
        )
        .await;
    assert_eq!(rsvp.status, StatusCode::OK, "{}", rsvp.text());
    assert_eq!(rsvp.json()["status"], "confirmed");
    assert!(
        h.state
            .app
            .db
            .audit_page(guild, None, 50)
            .await
            .unwrap()
            .iter()
            .any(|a| a.action == "reef.gathering.create")
    );
    h.mock.member_roles.lock().unwrap().insert(user, vec![]);
    assert_eq!(
        member.write(Method::POST, &edit, body).await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        member.get(&format!("/api/reef/moderation?guild={guild}")).await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        member.write(Method::POST, &gatherings, schedule).await.status,
        StatusCode::FORBIDDEN
    );
    h.mock.absent_leveling_members.lock().unwrap().insert((guild, user));
    assert_eq!(member.get(&path).await.status, StatusCode::FORBIDDEN);
    let csrf = manager.csrf.take();
    assert_eq!(
        manager.write(Method::POST, &gatherings, json!({})).await.status,
        StatusCode::FORBIDDEN
    );
    manager.csrf = csrf;
}
