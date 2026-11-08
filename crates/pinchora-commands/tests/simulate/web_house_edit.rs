use super::*;

#[tokio::test]
async fn web_edit_then_discord_undo_in_both_languages() {
    let Some(mut sim) = Sim::new("web_edit_then_discord_undo").await else {
        return;
    };
    for (lang, label) in [
        (pinchora_core::i18n::Lang::En, "↶ Undo"),
        (pinchora_core::i18n::Lang::Tr, "↶ Geri al"),
    ] {
        let user = unique();
        sim.app.set_user_lang(user, Some(lang)).await.unwrap();
        let _ = sim.command(user, "house", json!([{"name":"edit","type":1}])).await;
        let item = "plainwhorl_chair__sand";
        sim.app.db.grant_item(user, item, 1).await.unwrap();
        let rows = sim.app.db.placements(user, 0).await.unwrap();
        let result = pinchora_commands::house_edit::apply(
            &sim.app,
            pinchora_commands::Who::own(user),
            0,
            (8, 8),
            &pinchora_commands::house_edit::fingerprint(&rows),
            pinchora_commands::house_edit::Operation::Place {
                item_id: item.into(),
                x: 0,
                y: 0,
                flipped: false,
            },
        )
        .await
        .unwrap();
        assert!(matches!(result, pinchora_commands::house_edit::Outcome::Done(_)));
        let opened = sim.command(user, "house", json!([{"name":"edit","type":1}])).await;
        assert!(!opened.button(label).disabled);
        let _ = sim.click(user, &opened, label).await;
        assert!(sim.app.db.placements(user, 0).await.unwrap().is_empty());
        assert!(sim.app.db.owns(user, item).await.unwrap());
    }
}
