use crate::*;

const USE_APPLICATION_COMMANDS: u64 = 1 << 31;

#[tokio::test]
async fn command_control_switch_suspends_saved_slash_and_live_text_overrides() {
    use pinchora_core::admin::modules::Module;
    let Some(mut sim) = Sim::new("text_command_control_override_switch").await else {
        return;
    };
    let user = unique();
    sim.permissions = USE_APPLICATION_COMMANDS;
    sim.channel_is(sim.channel, 0, None);
    sim.app
        .db
        .save_command_permissions(
            sim.guild,
            APPLICATION_ID,
            &json!({
                "version":1,"ids":{"bio":51},"entries":[{
                    "id":"51","application_id":APPLICATION_ID.to_string(),"guild_id":sim.guild.to_string(),
                    "permissions":[{"id":user.to_string(),"type":2,"permission":false}]
                }]
            }),
        )
        .await
        .unwrap();
    sim.registered(&[(52, "profile")]);
    sim.integrations(&[(52, &[(user, 2, false)])]);
    assert!(
        sim.command(
            user,
            "customize",
            json!([sub("bio", vec![text_option("text", "Coast")])])
        )
        .await
        .content()
        .contains("server")
    );
    let denied = sim.text_one(user, &[], "p!profile", &[]).await;
    assert!(denied.content().contains("command permissions"), "{denied}");
    sim.app
        .db
        .set_module(sim.guild, Module::CommandControl, Some(false), None)
        .await
        .unwrap();
    sim.app.announce_guild_change(sim.guild).await;
    assert!(
        !sim.command(
            user,
            "customize",
            json!([sub("bio", vec![text_option("text", "Coast")])])
        )
        .await
        .content()
        .contains("server")
    );
    let allowed = sim.text_one(user, &[], "p!profile", &[]).await;
    assert!(!allowed.content().contains("command permissions"), "{allowed}");
    sim.app
        .db
        .set_module(sim.guild, Module::CommandControl, Some(true), None)
        .await
        .unwrap();
    sim.app.announce_guild_change(sim.guild).await;
    assert!(
        sim.command(
            user,
            "customize",
            json!([sub("bio", vec![text_option("text", "Coast")])])
        )
        .await
        .content()
        .contains("server")
    );
    let denied_again = sim.text_one(user, &[], "p!profile", &[]).await;
    assert!(denied_again.content().contains("command permissions"), "{denied_again}");
}
