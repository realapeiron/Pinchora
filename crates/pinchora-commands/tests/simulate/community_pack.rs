//! Member and manager Community flows in both supported languages.
use super::*;

#[tokio::test]
async fn member_suggests_and_staff_approves_qotd() {
    let Some(mut sim) = Sim::new("member_suggests_and_staff_approves_qotd").await else {
        return;
    };
    let member = unique();
    let staff = unique();
    let suggested = sim
        .command(
            member,
            "qotd",
            json!([sub("suggest", vec![text_option("question", "What made you smile?")])]),
        )
        .await;
    assert!(
        suggested.ephemeral && suggested.content().contains("review queue"),
        "{suggested}"
    );
    sim.permissions = twilight_model::guild::Permissions::MANAGE_GUILD.bits();
    let queue = sim
        .command(staff, "community", json!([group("qotd", sub("queue", vec![]))]))
        .await;
    assert!(
        queue.ephemeral && queue.content().contains("What made you smile?"),
        "{queue}"
    );
    let id = sim.app.db.qotd_queue(sim.guild).await.unwrap()[0].id;
    let decided = sim.component(staff, &format!("qd:ok:{id}"), &[], &queue).await;
    assert!(decided.ephemeral && decided.content().contains("updated"), "{decided}");
    assert!(sim.app.db.qotd_queue(sim.guild).await.unwrap()[0].approved);
}

#[tokio::test]
async fn counting_and_forms_reply_in_turkish() {
    let Some(mut sim) = Sim::new("counting_and_forms_reply_in_turkish").await else {
        return;
    };
    sim.locale = "tr";
    let member = unique();
    let stats = sim.command(member, "counting", json!([sub("stats", vec![])])).await;
    assert!(stats.ephemeral && stats.content().contains("Rekor"), "{stats}");
    let form = sim.command(member, "form", json!([sub("list", vec![])])).await;
    assert!(
        form.ephemeral && form.content().contains("Açık başvuru formu yok"),
        "{form}"
    );
}
