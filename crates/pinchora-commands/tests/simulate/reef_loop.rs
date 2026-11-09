//! Every command in both languages, using the real dispatch and PNG rendering.
use crate::*;
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
fn sub(name: &str, options: Value) -> Value {
    json!([{"name":name,"type":1,"options":options}])
}
async fn flow(locale: &str, name: &'static str) {
    let Some(mut sim) = Sim::new(name).await else { return };
    sim.locale = if locale == "tr" { "tr" } else { "en-US" };
    let lang = if locale == "tr" { Lang::Tr } else { Lang::En };
    let u = unique();
    let v = unique();
    fund(&sim, u, 10000).await;
    fund(&sim, v, 1000).await;
    let db = sim.app.db.clone();
    let now = sim.app.now();
    for (root, action) in [
        ("fish", "dock"),
        ("fish", "contest"),
        ("fish", "season"),
        ("fish", "dive"),
        ("house", "aquarium"),
        ("house", "garden"),
        ("house", "craft"),
        ("pet", "streak"),
    ] {
        let reply = sim.command(u, root, sub(action, json!([]))).await;
        assert!(reply.ephemeral, "{root} {action}");
        assert!(!reply.files.is_empty(), "{root} {action}: {}", reply.description());
    }
    let planted = sim
        .command(
            u,
            "house",
            sub("garden", json!([{"name":"action","type":3,"value":"plant"}])),
        )
        .await;
    assert!(planted.description().contains(&t!(lang, "reef.done")));
    sqlx::query("UPDATE reef_loop SET planted_at=$2 WHERE user_id=$1")
        .bind(u as i64)
        .bind(now - chrono::Duration::hours(9))
        .execute(db.pool())
        .await
        .unwrap();
    for action in ["water", "harvest"] {
        let reply = sim
            .command(
                u,
                "house",
                sub("garden", json!([{"name":"action","type":3,"value":action}])),
            )
            .await;
        assert!(reply.description().contains(&t!(lang, "reef.done")));
    }
    db.record_catch(
        u,
        pinchora_db::NewCatch {
            fish_id: "toaststripe_minnow",
            size_cm: 7.,
            ratio: 1.2,
            weekly: Some((sim.guild, now.date_naive())),
            milestones: &[],
            known: &["toaststripe_minnow"],
        },
    )
    .await
    .unwrap();
    let id = db.bucket(u).await.unwrap()[0].id;
    let tank=sim.command(u,"house",sub("aquarium",json!([{"name":"action","type":3,"value":"add"},{"name":"fish","type":4,"value":id},{"name":"slot","type":4,"value":1}]))).await;
    assert!(tank.description().contains(&t!(lang, "reef.done")));
    let renamed = sim
        .command(
            u,
            "house",
            sub(
                "aquarium",
                json!([{"name":"action","type":3,"value":"rename"},{"name":"name","type":3,"value":"Mavi Yuva"}]),
            ),
        )
        .await;
    assert!(!renamed.files.is_empty());
    let home = sim.command(u, "house", sub("view", json!([]))).await;
    assert_eq!(home.files.len(), 2);
    let contest = sim.command(u, "fish", sub("contest", json!([]))).await;
    assert!(!contest.files.is_empty());
    let start = sim
        .command(u, "fish", sub("dive", json!([{"name":"start","type":5,"value":true}])))
        .await;
    let intruder = sim.click(v, &start, &t!(lang, "reef.surface")).await;
    assert!(intruder.content().contains(&t!(lang, "reef.stale")));
    let surface = sim.click(u, &start, &t!(lang, "reef.surface")).await;
    assert!(surface.description().contains(&t!(lang, "reef.done")));
    let stale = sim.click(u, &start, &t!(lang, "reef.surface")).await;
    assert!(stale.description().contains(&t!(lang, "reef.stale")));
    db.pair_partners(u, v).await.unwrap();
    for user in [u, v] {
        let reply = sim.command(user, "partner", sub("streak", json!([]))).await;
        assert!(!reply.files.is_empty());
    }
    let pet = db.adopt(u, "crab", "Pebble").await.unwrap();
    db.save_pet(u, &pet, [95, 95, 95, 95], 1, 5, now).await.unwrap();
    let care = sim.command(u, "pet", sub("streak", json!([]))).await;
    assert_eq!(care.title(), t!(lang, "reef.pet-title"));
    for _ in 0..2 {
        db.record_catch(
            u,
            pinchora_db::NewCatch {
                fish_id: "toaststripe_minnow",
                size_cm: 5.,
                ratio: 1.,
                weekly: None,
                milestones: &[],
                known: &["toaststripe_minnow"],
            },
        )
        .await
        .unwrap();
    }
    sqlx::query("UPDATE reef_loop SET seaweed=20 WHERE user_id=$1")
        .bind(u as i64)
        .execute(db.pool())
        .await
        .unwrap();
    db.earn_pearls(u, 3).await.unwrap();
    let crafted = sim
        .command(
            u,
            "house",
            sub("craft", json!([{"name":"recipe","type":3,"value":"sprig"}])),
        )
        .await;
    assert!(crafted.description().contains(&t!(lang, "reef.done")));
    // Every possible all-day dock request is in the bucket; the handler chooses this rotation.
    for species in Content::get().fish.iter().filter(|f| {
        !f.junk && f.rarity == pinchora_game::Rarity::Common && f.time == "any" && f.spots.contains_key("shore")
    }) {
        for _ in 0..2 {
            db.record_catch(
                u,
                pinchora_db::NewCatch {
                    fish_id: &species.id,
                    size_cm: species.min_cm,
                    ratio: 1.,
                    weekly: None,
                    milestones: &[],
                    known: &[&species.id],
                },
            )
            .await
            .unwrap();
        }
    }
    let delivered = sim
        .command(
            u,
            "fish",
            sub("dock", json!([{"name":"deliver","type":5,"value":true}])),
        )
        .await;
    assert!(delivered.description().contains(&t!(lang, "reef.done")));
    let duplicate = sim
        .command(
            u,
            "fish",
            sub("dock", json!([{"name":"deliver","type":5,"value":true}])),
        )
        .await;
    assert!(duplicate.description().contains(&t!(lang, "reef.already")));
    let previous = pinchora_core::games::reef_loop::season_start(
        pinchora_core::games::reef_loop::season_start(now.date_naive()) - chrono::Duration::days(1),
    );
    for day in 0..3 {
        sqlx::query("INSERT INTO reef_catches(guild_id,day,user_id,fish_id,size_cm,score) VALUES($1,$2,$3,'toaststripe_minnow',7,1.2)").bind(sim.guild as i64).bind(previous+chrono::Duration::days(day)).bind(u as i64).execute(db.pool()).await.unwrap();
    }
    let result = sim
        .command(
            u,
            "fish",
            sub("season", json!([{"name":"claim","type":5,"value":true}])),
        )
        .await;
    assert!(result.description().contains(&t!(lang, "reef.done")));
    assert!(db.owns(u, "reefbubble_lamp__sky~bound").await.unwrap());
    let diver = unique();
    fund(&sim, diver, 300).await;
    let start = sim
        .command(
            diver,
            "fish",
            sub("dive", json!([{"name":"start","type":5,"value":true}])),
        )
        .await;
    let descent = sim.click(diver, &start, &t!(lang, "reef.push")).await;
    assert!(
        descent.description().contains(&t!(lang, "reef.done"))
            || descent.description().contains(&t!(lang, "reef.lost"))
    );
}
#[tokio::test]
async fn reef_loop_english() {
    Box::pin(flow("en", "reef_loop_english")).await;
}
#[tokio::test]
async fn reef_loop_turkish() {
    Box::pin(flow("tr", "reef_loop_turkish")).await;
}

#[tokio::test]
async fn aquarium_navigation_rechecks_locked_and_blocked_house_access() {
    let Some(mut sim) = Sim::new("aquarium_navigation_access").await else {
        return;
    };
    let owner = unique();
    let visitor = unique();
    let db = sim.app.db.clone();
    fund(&sim, owner, 10000).await;
    fund(&sim, visitor, 1000).await;
    sim.command(owner, "house", sub("aquarium", json!([]))).await;
    sqlx::query("INSERT INTO reef_aquarium(user_id,slot,fish_id,size_cm) VALUES($1,1,'toaststripe_minnow',5)")
        .bind(owner as i64)
        .execute(db.pool())
        .await
        .unwrap();
    let home = sim.command(owner, "house", sub("view", json!([]))).await;
    assert_eq!(home.files.len(), 2);
    let nav = format!("hv:{owner}:0");
    let open = sim.component(visitor, &nav, &[], &home).await;
    assert_eq!(open.files.len(), 2);
    db.set_house_locked(owner, true).await.unwrap();
    let locked = sim.component(visitor, &nav, &[], &home).await;
    assert!(locked.ephemeral && locked.files.is_empty());
    assert_eq!(locked.content(), t!(Lang::En, "privacy.house-private"));
    db.set_house_locked(owner, false).await.unwrap();
    db.block_signer(owner, visitor, "visitor").await.unwrap();
    let blocked = sim.component(visitor, &nav, &[], &home).await;
    assert!(blocked.ephemeral && blocked.files.is_empty());
    assert_eq!(blocked.content(), t!(Lang::En, "privacy.house-private"));
    let own = sim.component(owner, &nav, &[], &home).await;
    assert_eq!(own.files.len(), 2);
}
