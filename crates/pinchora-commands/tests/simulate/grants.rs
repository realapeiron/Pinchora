//! Who may set a role up to be handed out: the server owner any role the bot can give, an administrator
//! the ones below their own highest role, a manager only those below it without a dangerous permission.

use super::*;

const ADMINISTRATOR: u64 = 1 << 3;
const MANAGE_MESSAGES: u64 = 1 << 13;

fn level_role(role: &Value) -> Value {
    json!([group(
        "level-roles",
        sub("add", vec![int_option("level", 2), role_option("role", role)])
    )])
}

fn store_role(role: &Value) -> Value {
    json!([group(
        "store",
        sub("add", vec![role_option("role", role), int_option("price", 10)])
    )])
}

fn join_role(role: &Value) -> Value {
    json!([sub("join-role", vec![role_option("role", role)])])
}

/// Who runs it, their permissions, their roles, the command, the role, and what the answer says.
type Case<'a> = (u64, u64, Vec<u64>, fn(&Value) -> Value, &'a Value, &'a str);

#[tokio::test]
async fn only_the_owner_or_an_admin_sets_up_powerful_roles_and_nobody_above_themselves() {
    let Some(mut sim) = Sim::new("only_the_owner_or_an_admin_sets_up_powerful_roles_and_nobody_above_themselves").await
    else {
        return;
    };
    let (owner, staff) = (unique(), unique());
    sim.own_server(owner);
    let (fan_id, helper_id, lead_id, mods_id) = (unique(), unique(), unique(), unique());
    let fan = sim.guild_role(fan_id, "Fan", 2);
    let helper = sim.guild_role_with(helper_id, "Helper", 3, MANAGE_MESSAGES);
    let lead = sim.guild_role(lead_id, "Lead", 5);
    let mods = sim.guild_role_with(mods_id, "Mods", 6, ADMINISTRATOR);

    let powers = "has Manage Messages, so only the server owner or an administrator";
    let not_below = "is not below your highest role";
    let cases: [Case; 9] = [
        (staff, MANAGE_GUILD, vec![lead_id], level_role, &fan, "level **2**"),
        (staff, MANAGE_GUILD, vec![lead_id], level_role, &helper, powers),
        (staff, MANAGE_GUILD, vec![lead_id], level_role, &lead, not_below),
        (staff, MANAGE_GUILD, vec![lead_id], store_role, &helper, powers),
        (staff, MANAGE_GUILD, vec![lead_id], join_role, &helper, powers),
        (staff, MANAGE_GUILD, vec![], join_role, &fan, not_below),
        (
            staff,
            MANAGE_GUILD | ADMINISTRATOR,
            vec![lead_id],
            store_role,
            &helper,
            "On sale now",
        ),
        (
            staff,
            MANAGE_GUILD | ADMINISTRATOR,
            vec![lead_id],
            level_role,
            &mods,
            not_below,
        ),
        (owner, MANAGE_GUILD, vec![], level_role, &mods, "level **2**"),
    ];
    for (n, (who, permissions, roles, command, role, says)) in cases.into_iter().enumerate() {
        sim.permissions = permissions;
        sim.roles = roles;
        let reply = sim
            .command_resolved(who, "config", command(role), resolved(&[], &[role]))
            .await;
        assert!(reply.ephemeral, "case {n}");
        assert!(reply.content().contains(says), "case {n}: {}", reply.content());
    }
    let settings = sim.app.db.guild_settings(sim.guild).await.unwrap();
    assert_eq!(settings.join_role_id, None, "no refused join role was saved");
    let levels: Vec<u64> = sim
        .app
        .db
        .level_roles(sim.guild)
        .await
        .unwrap()
        .iter()
        .map(|r| r.role_id as u64)
        .collect();
    assert_eq!(levels.len(), 2, "@Fan by the manager, @Mods by the owner: {levels:?}");
    assert!(levels.contains(&fan_id) && levels.contains(&mods_id));
}
