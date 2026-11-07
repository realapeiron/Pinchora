//! The manager flow and public paged card through the real handler.

use super::*;

#[tokio::test]
async fn achievements_are_managed_granted_once_and_paged() {
    let Some(mut sim) = Sim::new("achievements_are_managed_granted_once_and_paged").await else {
        return;
    };
    let (manager, member) = (unique(), unique());
    sim.permissions = MANAGE_GUILD;
    for n in 0..6 {
        let name = format!("Pearl {n}");
        let saved = sim
            .command(
                manager,
                "achievementadmin",
                json!([sub(
                    "add",
                    vec![
                        text_option("name", &name),
                        text_option("kind", "custom"),
                        int_option("target", 1),
                        int_option("points", 25),
                    ]
                )]),
            )
            .await;
        assert!(
            saved.ephemeral && saved.content().contains("saved"),
            "{}",
            saved.content()
        );
    }
    let rows = sim.app.db.achievements(sim.guild).await.unwrap();
    let first = rows[0].id;
    let granted = sim
        .command_resolved(
            manager,
            "achievementadmin",
            json!([sub(
                "grant",
                vec![int_option("id", first), user_option("user", member),]
            )]),
            resolved(&[member], &[]),
        )
        .await;
    assert!(granted.ephemeral && granted.content().contains("Granted"));
    assert_eq!(sim.app.db.member_points(sim.guild, member).await.unwrap().0, 25);
    let again = sim
        .command_resolved(
            manager,
            "achievementadmin",
            json!([sub(
                "grant",
                vec![int_option("id", first), user_option("user", member),]
            )]),
            resolved(&[member], &[]),
        )
        .await;
    assert!(again.content().contains("already earned"));
    assert_eq!(sim.app.db.member_points(sim.guild, member).await.unwrap().0, 25);

    sim.permissions = 0;
    let card = sim
        .command_resolved(
            member,
            "achievements",
            json!([user_option("user", member)]),
            resolved(&[member], &[]),
        )
        .await;
    assert!(
        !card.ephemeral
            && card
                .files
                .iter()
                .any(|(name, bytes)| name == "achievements.png" && bytes.len() > 1000)
    );
    assert_eq!(card.rows(), 1);
    let next = sim.click(member, &card, "Next").await;
    assert!(next.files.iter().any(|(name, _)| name == "achievements.png"));
    assert!(next.button("Next").disabled);
    for options in [
        json!([sub(
            "add",
            vec![
                text_option("name", "No"),
                text_option("kind", "custom"),
                int_option("target", 1)
            ]
        )]),
        json!([sub("edit", vec![int_option("id", first), text_option("name", "No")])]),
        json!([sub("remove", vec![int_option("id", first)])]),
        json!([sub("list", vec![])]),
        json!([sub("grant", vec![int_option("id", first), user_option("user", member)])]),
    ] {
        let denied = sim
            .command_resolved(member, "achievementadmin", options, resolved(&[member], &[]))
            .await;
        assert!(denied.ephemeral && denied.content().contains("Manage Server"));
    }
}
