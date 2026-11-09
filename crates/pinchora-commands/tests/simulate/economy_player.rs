use crate::*;
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
fn sub(name: &str, options: Value) -> Value {
    json!([{"name":name,"type":1,"options":options}])
}
async fn flow(locale: &'static str, name: &'static str) {
    let Some(mut sim) = Sim::new(name).await else { return };
    sim.locale = locale;
    let l = if locale == "tr" { Lang::Tr } else { Lang::En };
    let u = unique();
    let v = unique();
    fund(&sim, u, 20000).await;
    fund(&sim, v, 1000).await;
    for leaf in ["jobs", "provisions", "chest", "season", "prestige", "donate"] {
        let r = sim.command(u, "rewards", sub(leaf, json!([]))).await;
        assert!(r.ephemeral);
        assert!(!r.description().is_empty());
    }
    let start = sim
        .command(
            u,
            "rewards",
            sub("jobs", json!([{"name":"job","type":3,"value":"kelp_keeper"}])),
        )
        .await;
    let denied = sim.click(v, &start, &t!(l, "eco.task_kelp_keeper_0")).await;
    assert!(denied.content().contains(&t!(l, "eco.error_forbidden")));
    let mut r = start;
    for i in 0..3 {
        r = sim.click(u, &r, &t!(l, &format!("eco.task_kelp_keeper_{i}"))).await;
    }
    assert!(r.description().contains(&t!(l, "eco.shift_paid", amount = 100)));
    let balance = sim.command(u, "balance", json!([])).await;
    assert!(!balance.files.is_empty());
    let db = sim.app.db.clone();
    let time = sim.app.now();
    let tend = sim
        .command(
            u,
            "rewards",
            sub("provisions", json!([{"name":"action","type":3,"value":"tend"}])),
        )
        .await;
    assert!(tend.description().contains(&t!(l, "eco.done", amount = 0, id = 0)));
    sqlx::query(
        "UPDATE economy_players SET provision_at=$2,chests=1,season=$3,season_xp=50,shifts=40 WHERE user_id=$1",
    )
    .bind(u as i64)
    .bind(time - chrono::Duration::hours(12))
    .bind(pinchora_core::economy::player::season(time))
    .execute(db.pool())
    .await
    .unwrap();
    for (leaf, options) in [
        ("provisions", json!([{"name":"action","type":3,"value":"claim"}])),
        ("chest", json!([{"name":"open","type":5,"value":true}])),
        ("season", json!([{"name":"tier","type":4,"value":1}])),
        ("prestige", json!([{"name":"confirm","type":5,"value":true}])),
        ("donate", json!([{"name":"amount","type":4,"value":100}])),
    ] {
        let result = sim.command(u, "rewards", sub(leaf, options)).await;
        assert_eq!(result.title(), t!(l, "eco.title"), "{leaf}: {}", result.content());
    }
    assert_eq!(db.economy_settings(sim.guild).await.unwrap()["pool_total"], 100);
    let item = Content::get().items().find(|i| !i.bound).unwrap().id();
    sqlx::query("INSERT INTO inventory(user_id,item_id,quantity) VALUES($1,$2,1)")
        .bind(u as i64)
        .bind(&item)
        .execute(db.pool())
        .await
        .unwrap();
    let listed=sim.command(u,"stall",sub("auction",json!([{"name":"item","type":3,"value":item},{"name":"price","type":4,"value":100},{"name":"hours","type":4,"value":1}]))).await;
    assert_eq!(listed.title(), t!(l, "eco.title"));
    let seller_id = u.to_string();
    let auction = db.economy_snapshot(u, time).await.unwrap()["auctions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["seller_id"].as_str() == Some(seller_id.as_str()))
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    let bid = sim
        .command(
            v,
            "stall",
            sub(
                "bid",
                json!([{"name":"id","type":4,"value":auction},{"name":"amount","type":4,"value":100}]),
            ),
        )
        .await;
    assert_eq!(bid.title(), t!(l, "eco.title"));
    let cancel = sim
        .command(
            u,
            "stall",
            sub(
                "auction-close",
                json!([{"name":"id","type":4,"value":auction},{"name":"cancel","type":5,"value":true}]),
            ),
        )
        .await;
    assert!(cancel.content().contains(&t!(l, "eco.error_auction_cancel")));
    sqlx::query("UPDATE economy_auctions SET ends_at=$2 WHERE id=$1")
        .bind(auction)
        .bind(time - chrono::Duration::seconds(1))
        .execute(db.pool())
        .await
        .unwrap();
    let settled = sim
        .command(
            u,
            "stall",
            sub("auction-close", json!([{"name":"id","type":4,"value":auction}])),
        )
        .await;
    assert_eq!(settled.title(), t!(l, "eco.title"));
    let created=sim.command_resolved(u,"stall",sub("trade",json!([{"name":"user","type":6,"value":v.to_string()},{"name":"give","type":4,"value":100},{"name":"receive","type":4,"value":50}])),resolved(&[v],&[])).await;
    assert_eq!(created.title(), t!(l, "eco.trades"));
    let id = db.economy_snapshot(u, time).await.unwrap()["trades"][0]["id"]
        .as_i64()
        .unwrap();
    let a = sim
        .command(
            u,
            "stall",
            sub("trade-review", json!([{"name":"id","type":4,"value":id}])),
        )
        .await;
    assert!(a.description().contains(&v.to_string()));
    let _ = sim.click(u, &a, &t!(l, "eco.confirm")).await;
    let b = sim
        .command(
            v,
            "stall",
            sub("trade-review", json!([{"name":"id","type":4,"value":id}])),
        )
        .await;
    let final_reply = sim.click(v, &b, &t!(l, "eco.confirm")).await;
    assert!(final_reply.description().contains(&t!(l, "eco.status_completed")));
    let denied=sim.command(u,"progression",json!([{"name":"economy","type":2,"options":[{"name":"tools","type":1,"options":[{"name":"action","type":3,"value":"scale"},{"name":"percent","type":4,"value":0},{"name":"version","type":3,"value":"fake"}]}]}])).await;
    assert!(denied.ephemeral);
    assert!(!denied.content().is_empty());
    sim.permissions = MANAGE_GUILD;
    let saved=sim.command(u,"progression",json!([{"name":"economy","type":2,"options":[{"name":"tools","type":1,"options":[{"name":"action","type":3,"value":"save"},{"name":"goal","type":4,"value":20000}]}]}])).await;
    assert_eq!(saved.title(), t!(l, "eco.tools"));
    assert_eq!(db.economy_settings(sim.guild).await.unwrap()["pool_goal"], 20000);
}
#[tokio::test]
async fn english_economy_flow() {
    flow("en-US", "economy_v2_en").await
}
#[tokio::test]
async fn turkish_economy_flow() {
    flow("tr", "economy_v2_tr").await
}
