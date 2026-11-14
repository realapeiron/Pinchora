use super::*;

#[tokio::test]
async fn pet_care_ignores_another_tests_cooldown() {
    let Some(mut sim) = Sim::new("pet_care_ignores_another_tests_cooldown").await else {
        return;
    };
    let user = unique();
    let pet = sim.app.db.adopt(user, "crumbclaw_crab", "Pip").await.unwrap();
    set_pet(&sim.app.db, user, pet.id, fresh_needs(40), 29, 1_215, sim.app.now())
        .await
        .unwrap();
    let card = sim.command(user, "pet", pet_command("view")).await;

    // A different test database can allocate this same pet id while sharing Redis database 12.
    let key = format!("pinchora:cd:pet:{}:Clean", pet.id);
    let _: () = redis::cmd("SET")
        .arg(&key)
        .arg(1)
        .arg("EX")
        .arg(60)
        .query_async(&mut sim.app.cache.conn())
        .await
        .unwrap();
    let bathed = sim.click(user, &card, "Bath").await;
    let _: () = redis::cmd("DEL")
        .arg(&key)
        .query_async(&mut sim.app.cache.conn())
        .await
        .unwrap();
    assert!(bathed.description().contains("Pip reached level 30!"), "{bathed}");
}
