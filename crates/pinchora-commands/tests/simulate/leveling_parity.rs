use super::*;
use pinchora_core::leveling::week_start;

#[tokio::test]
async fn curve_tuning_and_voice_rank_work_in_english_and_turkish() {
    let Some(mut sim) = Sim::new("leveling_parity_curve_voice_rank").await else {
        return;
    };
    sim.permissions = MANAGE_GUILD;
    let (admin, member, channel) = (unique(), unique(), unique());
    let response = sim
        .xp(
            admin,
            sub(
                "curve",
                vec![
                    text_option("kind", "linear"),
                    int_option("base", 100),
                    int_option("linear", 20),
                ],
            ),
        )
        .await;
    assert!(response.ephemeral && response.content().contains("saved"), "{response}");
    let response = sim
        .xp(
            admin,
            sub(
                "tuning",
                vec![int_option("min", 40), int_option("max", 40), int_option("cooldown", 5)],
            ),
        )
        .await;
    assert!(response.ephemeral && response.content().contains("saved"), "{response}");
    sim.say_in(member, channel, &[]).await;
    assert_eq!(sim.server_xp(member).await, 40);
    assert_eq!(sim.app.db.leveling_parity(sim.guild).await.unwrap().xp_to_next(2), 140);
    sim.locale = "tr";
    let response = sim
        .xp(
            admin,
            sub(
                "sources",
                vec![
                    bool_option("reactions", true),
                    bool_option("separate_voice_level", true),
                ],
            ),
        )
        .await;
    assert!(
        response.ephemeral && response.content().contains("kaydedildi"),
        "{response}"
    );
    let gain = sim
        .app
        .db
        .add_separate_voice_xp(sim.guild, &[(member, 150)], week_start(sim.app.now()))
        .await
        .unwrap();
    assert_eq!(gain[0].1.new_xp, 150);
    assert_eq!(
        sim.server_xp(member).await,
        40,
        "separate voice XP leaves chat XP alone"
    );
    let rank = sim.command(member, "rank", json!([bool_option("voice", true)])).await;
    assert_eq!(rank.files.len(), 1, "{rank}");
    let board = sim
        .command(member, "leaderboard", json!([text_option("board", "voice")]))
        .await;
    assert_eq!(board.files.len(), 1, "{board}");
}
