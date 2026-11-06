//! The everyday commands through the real handlers: ping and help, bio, rep and give, the shops,
//! gifts, stalls, the plaza, quests, the house commands and the first /config settings, and a game of
//! tic-tac-toe that nobody wins. Pictures land in `target/sim/<test>/`.

use pinchora_game::{Availability, Category, Content, house};
use pinchora_render::thousands;
use twilight_model::channel::message::Component;

use super::*;

/// A picture's size, once it is seen to be a PNG.
fn size_of(reply: &Reply) -> (u32, u32) {
    let (name, bytes) = reply.files.first().expect("a picture");
    let image = image::load_from_memory(bytes).unwrap_or_else(|e| panic!("{name} is not a png: {e}"));
    (image.width(), image.height())
}

/// The id of a piece of furniture that is sold in the catalog, by position.
fn catalog_piece(n: usize) -> String {
    Content::get()
        .items()
        .filter(|i| {
            i.base.availability == Availability::Permanent && i.base.currency == pinchora_game::Currency::Shells
        })
        .nth(n)
        .map(|i| i.id())
        .expect("a catalog piece")
}

/// Pieces that cannot be bought, so a forged purchase of one can be refused.
fn rotating_piece() -> String {
    Content::get()
        .items()
        .find(|i| i.base.availability != Availability::Permanent)
        .map(|i| i.id())
        .expect("a piece that is not in the catalog")
}

fn bot_resolved(bot: u64) -> Value {
    json!({ "users": { bot.to_string(): person_json(bot, "robot", Some("Robot"), true) } })
}

// /Ping and help

#[tokio::test]
async fn ping_answers_and_help_pages_through_every_area() {
    let Some(mut sim) = Sim::new("ping_answers_and_help_pages_through_every_area").await else {
        return;
    };
    let user = unique();

    let pong = sim.command(user, "ping", json!([])).await;
    assert_eq!(pong.title(), "Pong!");
    assert!(!pong.ephemeral, "a ping is for the channel");
    let fields: Vec<&str> = pong.embeds[0].fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(fields, ["Interaction", "Database", "Redis"]);
    assert!(
        pong.embeds[0].footer.as_ref().is_some_and(|f| f.text == "Node: sim"),
        "{:?}",
        pong.embeds[0].footer
    );

    // The help is a private card; its menu names every page, and Next walks through all of them.
    let first = sim.command(user, "help", json!([])).await;
    assert!(first.ephemeral && first.title() == "Pinchora", "{}", first.title());
    let areas = help_areas(&first);
    assert!(
        (15..=25).contains(&areas.len()),
        "the guide has {} pages: {areas:?}",
        areas.len()
    );
    assert_eq!(size_of(&first).0, 900);
    assert!(first.button("◀ Previous").disabled && !first.button("Next ▶").disabled);
    let chosen = |reply: &Reply| {
        reply
            .options("hp:s")
            .iter()
            .position(|o| o.default)
            .expect("a page is chosen")
    };
    assert_eq!(chosen(&first), 0);

    let mut page = first.clone();
    let mut heights = vec![size_of(&page).1];
    for n in 1..areas.len() {
        page = sim.click(user, &page, "Next ▶").await;
        assert_eq!(chosen(&page), n, "the menu follows the page");
        assert_eq!(size_of(&page).0, 900);
        heights.push(size_of(&page).1);
        assert_eq!(page.button("Next ▶").disabled, n + 1 == areas.len());
    }
    assert!(page.button("Next ▶").disabled, "the last page has no next");
    assert!(heights.iter().all(|h| (250..=900).contains(h)), "{heights:?}");

    // Back one page, then straight to a page by the menu; a page the guide does not have is the last.
    let back = sim.click(user, &page, "◀ Previous").await;
    assert_eq!(chosen(&back), areas.len() - 2);
    let house_page = areas
        .iter()
        .find(|a| a.starts_with("House & plaza"))
        .expect("a house page");
    let value = back
        .options("hp:s")
        .iter()
        .find(|o| &o.label == house_page)
        .expect("in the menu")
        .value
        .clone();
    let house = sim.pick(user, &back, "hp:s", &value).await;
    assert_eq!(&back.options("hp:s")[chosen(&house)].label, house_page);
    let far = sim.component(user, "hp:999", &[], &house).await;
    assert_eq!(chosen(&far), areas.len() - 1);
    let junk = sim.component(user, "hp:x", &[], &house).await;
    assert_eq!(chosen(&junk), 0, "an id that is not a page is the first page");
}

// /Bio, rep and give

#[tokio::test]
async fn bio_rep_and_give_follow_their_rules() {
    let Some(mut sim) = Sim::new("bio_rep_and_give_follow_their_rules").await else {
        return;
    };
    let (a, b, bot) = (unique(), unique(), unique());
    let db = sim.app.db.clone();

    // /bio keeps the words and drops the extra white space; a dash clears it.
    let set = sim
        .command(
            a,
            "bio",
            json!([text_option("text", "  Crab   enjoyer \n of tide pools ")]),
        )
        .await;
    assert!(
        set.ephemeral && set.content().contains("Bio updated"),
        "{}",
        set.content()
    );
    assert_eq!(db.user(a).await.unwrap().bio, "Crab enjoyer of tide pools");
    let card = sim.command(a, "profile", json!([])).await;
    assert_eq!(size_of(&card).0, 960);
    let cleared = sim.command(a, "bio", json!([text_option("text", "-")])).await;
    assert!(
        cleared.ephemeral && cleared.content().contains("Bio cleared"),
        "{}",
        cleared.content()
    );
    assert_eq!(db.user(a).await.unwrap().bio, "");

    // /rep: a point for someone else, once a day, never for yourself or a bot.
    let rep = |target| json!([user_option("user", target)]);
    let given = sim.command_resolved(a, "rep", rep(b), resolved(&[b], &[])).await;
    assert!(
        !given.ephemeral && given.description().contains("gave") && given.description().contains("**1**"),
        "{}",
        given.description()
    );
    let again = sim.command_resolved(a, "rep", rep(b), resolved(&[b], &[])).await;
    assert!(
        again.ephemeral && again.content().contains("again"),
        "{}",
        again.content()
    );
    let myself = sim.command_resolved(a, "rep", rep(a), resolved(&[a], &[])).await;
    assert!(myself.content().contains("Nice try"), "{}", myself.content());
    let robot = sim.command_resolved(a, "rep", rep(bot), bot_resolved(bot)).await;
    assert!(robot.content().contains("Bots can't"), "{}", robot.content());
    assert_eq!(db.user(b).await.unwrap().reputation, 1, "only the first point counted");

    // /give moves shells between wallets and refuses what it cannot do.
    fund(&sim, a, 100).await;
    let give = |target, amount| json!([user_option("user", target), int_option("amount", amount)]);
    let sent = sim.command_resolved(a, "give", give(b, 30), resolved(&[b], &[])).await;
    assert!(!sent.ephemeral, "a gift is seen by the channel");
    assert!(
        sent.description().contains("Sent **30** shells") && sent.description().contains("**70** left"),
        "{}",
        sent.description()
    );
    assert_eq!((shells(&sim, a).await, shells(&sim, b).await), (70, 30));
    let too_much = sim.command_resolved(a, "give", give(b, 500), resolved(&[b], &[])).await;
    assert!(
        too_much.ephemeral && too_much.content().contains("only have 70"),
        "{}",
        too_much.content()
    );
    let to_self = sim.command_resolved(a, "give", give(a, 5), resolved(&[a], &[])).await;
    assert!(to_self.content().contains("yourself"), "{}", to_self.content());
    let to_bot = sim.command_resolved(a, "give", give(bot, 5), bot_resolved(bot)).await;
    assert!(to_bot.content().contains("Bots have no use"), "{}", to_bot.content());
    assert_eq!(
        (shells(&sim, a).await, shells(&sim, b).await),
        (70, 30),
        "refusals cost nothing"
    );
    let wallet = sim.command(b, "balance", json!([])).await;
    assert_eq!(size_of(&wallet).0, 900);
}

// /Shop and /inventory

#[tokio::test]
async fn the_market_and_the_catalog_sell_furniture_and_the_inventory_shows_it() {
    let Some(mut sim) = Sim::new("the_market_and_the_catalog_sell_furniture_and_the_inventory_shows_it").await else {
        return;
    };
    let (user, broke) = (unique(), unique());
    fund(&sim, user, 200_000).await;
    let content = Content::get();

    // The hourly market: a card of the eight pieces, a menu to buy one, the price taken once.
    let market = sim.command(user, "shop", json!([sub("market", vec![])])).await;
    assert_eq!(market.title(), "Hourly market");
    assert!(!market.ephemeral);
    assert_eq!(size_of(&market), (1000, 600));
    let offers: Vec<String> = market.options("mk:").iter().map(|o| o.value.clone()).collect();
    assert!(offers.len() <= 8);
    if offers.is_empty() {
        println!("  the market is sold out this hour; its purchases are not tried");
    } else {
        let mut bought = None;
        for piece in &offers {
            let before = shells(&sim, user).await;
            let reply = sim.pick(user, &market, "mk:", piece).await;
            assert!(reply.ephemeral);
            if reply.content().starts_with("🛍️ Bought") {
                let spent = before - shells(&sim, user).await;
                assert!(
                    spent > 0 && reply.content().contains(&format!("for {}", thousands(spent))),
                    "{}",
                    reply.content()
                );
                bought = Some(piece.clone());
                break;
            }
            assert!(reply.content().contains("sold out"), "{}", reply.content());
        }
        let piece = bought.expect("some piece was in stock");
        assert_eq!(owned(&sim, user, &piece).await, 1);
        let poor = sim.pick(broke, &market, "mk:", &piece).await;
        assert!(
            poor.content().contains("you have 0") || poor.content().contains("sold out"),
            "{}",
            poor.content()
        );
        assert_eq!(owned(&sim, broke, &piece).await, 0, "nothing without the shells");
    }
    // An old market's menu is closed, and the hour is checked on the server.
    let stale = sim.component(user, "mk:1", &[&catalog_piece(0)], &market).await;
    assert!(stale.content().contains("has closed"), "{}", stale.content());

    // The catalog: one category at a time, eight to a page, the price from the content.
    let beds: Vec<_> = content
        .items()
        .filter(|i| i.base.category == Category::Beds && i.base.availability == Availability::Permanent)
        .collect();
    let catalog = sim
        .command(
            user,
            "shop",
            json!([sub("catalog", vec![text_option("category", "beds")])]),
        )
        .await;
    assert_eq!(catalog.title(), "Beds catalog");
    assert_eq!(size_of(&catalog), (1000, 600));
    assert_eq!(catalog.options("cb:beds").len(), beds.len().min(8));
    assert!(catalog.button("◀ Page").disabled);
    assert_eq!(catalog.button("Page ▶").disabled, beds.len() <= 8);
    if beds.len() > 8 {
        let page = sim.click(user, &catalog, "Page ▶").await;
        assert_eq!(page.title(), "Beds catalog");
        assert!(!page.button("◀ Page").disabled);
        assert_eq!(page.options("cb:beds").len(), (beds.len() - 8).min(8));
        let back = sim.click(user, &page, "◀ Page").await;
        assert!(back.button("◀ Page").disabled);
    }
    let cheap = catalog
        .options("cb:beds")
        .iter()
        .find(|o| !o.description.as_deref().unwrap_or_default().contains("pearls"))
        .expect("a bed for shells")
        .value
        .clone();
    let price = content.item(&cheap).expect("in the content").base.price;
    let before = shells(&sim, user).await;
    let bought = sim.pick(user, &catalog, "cb:beds", &cheap).await;
    assert!(
        bought.ephemeral && bought.content().starts_with("🛍️ Bought") && bought.content().contains("/house edit"),
        "{}",
        bought.content()
    );
    assert_eq!(
        before - shells(&sim, user).await,
        price,
        "the catalog price, nothing else"
    );
    assert_eq!(owned(&sim, user, &cheap).await, 1);
    let too_dear = sim.pick(broke, &catalog, "cb:beds", &cheap).await;
    assert!(too_dear.content().contains("you have 0"), "{}", too_dear.content());
    assert_eq!(owned(&sim, broke, &cheap).await, 0);
    // A forged pick: a piece that does not exist, or one the catalog does not sell.
    let unknown = sim.component(user, "cb:beds", &["no_such_piece"], &catalog).await;
    assert!(unknown.content().contains("Pick an item"), "{}", unknown.content());
    let rotating = sim.component(user, "cb:beds", &[&rotating_piece()], &catalog).await;
    assert!(rotating.content().contains("isn't sold"), "{}", rotating.content());
    assert_eq!(shells(&sim, user).await, before - price, "forgeries cost nothing");

    // The inventory lists what was bought, and pages once there are more than eight kinds.
    let inventory = sim.command(user, "inventory", json!([])).await;
    assert!(inventory.ephemeral && inventory.title() == "Your inventory");
    assert_eq!(size_of(&inventory), (1000, 600));
    assert!(inventory.components.is_empty(), "a few pieces fit one page");
    for n in 0..12 {
        sim.app.db.grant_item(user, &catalog_piece(40 + n), 1).await.unwrap();
    }
    let full = sim.command(user, "inventory", json!([])).await;
    assert!(full.button("◀ Page").disabled && !full.button("Page ▶").disabled);
    let second = sim.click(user, &full, "Page ▶").await;
    assert!(!second.button("◀ Page").disabled && second.button("Page ▶").disabled);
    assert_eq!(size_of(&second), (1000, 600));
}

// /Gift

#[tokio::test]
async fn a_gift_moves_one_piece_and_is_announced() {
    let Some(mut sim) = Sim::new("a_gift_moves_one_piece_and_is_announced").await else {
        return;
    };
    let (giver, friend, empty_handed) = (unique(), unique(), unique());
    let piece = catalog_piece(3);
    let name = Content::get().item(&piece).expect("in the content").name();
    sim.app.db.grant_item(giver, &piece, 1).await.unwrap();
    let gift = |to| json!([user_option("user", to)]);

    // Nothing to give: the command says where to get something.
    let nothing = sim
        .command_resolved(empty_handed, "gift", gift(friend), resolved(&[friend], &[]))
        .await;
    assert!(
        nothing.ephemeral && nothing.content().contains("inventory is empty"),
        "{}",
        nothing.content()
    );
    // Not to yourself, not to a robot.
    let myself = sim
        .command_resolved(giver, "gift", gift(giver), resolved(&[giver], &[]))
        .await;
    assert!(
        myself.content().contains("Gifts are for friends"),
        "{}",
        myself.content()
    );
    let robot = sim
        .command_resolved(giver, "gift", gift(friend), bot_resolved(friend))
        .await;
    assert!(robot.content().contains("Gifts are for friends"), "{}", robot.content());

    let menu = sim
        .command_resolved(giver, "gift", gift(friend), resolved(&[friend], &[]))
        .await;
    assert!(
        menu.ephemeral && menu.content().contains(&format!("<@{friend}>")),
        "{}",
        menu.content()
    );
    assert!(menu.options("gf:").iter().any(|o| o.value == piece && o.label == name));
    let sent = sim.pick(giver, &menu, "gf:", &piece).await;
    assert!(sent.content().contains("Sent"), "{}", sent.content());
    let news = sent.followups.first().expect("the channel is told");
    assert!(
        !news.ephemeral && news.content().contains(&name) && news.content().contains(&format!("<@{friend}>")),
        "{}",
        news.content()
    );
    assert!(
        news.pings_only(friend),
        "the friend is told, nobody else: {:?}",
        news.mentions
    );
    assert_eq!(
        (owned(&sim, giver, &piece).await, owned(&sim, friend, &piece).await),
        (0, 1)
    );
    // The menu is still on the screen: a second pick finds the shelf empty and moves nothing.
    let again = sim.pick(giver, &menu, "gf:", &piece).await;
    assert!(
        again.ephemeral && again.content().contains("anymore"),
        "{}",
        again.content()
    );
    assert_eq!(
        (owned(&sim, giver, &piece).await, owned(&sim, friend, &piece).await),
        (0, 1)
    );
    // A forged menu value is not a gift either.
    let forged = sim
        .component(giver, &format!("gf:{friend}"), &["no_such_piece"], &menu)
        .await;
    assert!(forged.content().contains("Pick something"), "{}", forged.content());
}

// /Stall

#[tokio::test]
async fn a_stall_lists_sells_and_gives_pieces_back() {
    let Some(mut sim) = Sim::new("a_stall_lists_sells_and_gives_pieces_back").await else {
        return;
    };
    let (seller, buyer, other) = (unique(), unique(), unique());
    let (chair, lamp) = (catalog_piece(5), catalog_piece(9));
    let db = sim.app.db.clone();
    db.grant_item(seller, &chair, 1).await.unwrap();
    db.grant_item(seller, &lamp, 1).await.unwrap();
    fund(&sim, buyer, 1_000).await;

    // An empty stall says so; a listing is a price and a piece.
    let empty = sim.command(seller, "stall", json!([sub("view", vec![])])).await;
    assert!(
        empty.description().contains("Nothing for sale"),
        "{}",
        empty.description()
    );
    assert_eq!(size_of(&empty), (1000, 600));
    let nothing = sim.command(seller, "stall", json!([sub("manage", vec![])])).await;
    assert!(
        nothing.ephemeral && nothing.content().contains("stall is empty"),
        "{}",
        nothing.content()
    );
    let menu = sim
        .command(seller, "stall", json!([sub("sell", vec![int_option("price", 200)])]))
        .await;
    assert!(
        menu.ephemeral && menu.content().contains("**200**") && menu.content().contains("190"),
        "the fee is spelled out: {}",
        menu.content()
    );
    let listed = sim.pick(seller, &menu, "sl:200", &chair).await;
    assert!(
        listed.content().contains("is on your stall for 200"),
        "{}",
        listed.content()
    );
    assert_eq!(
        owned(&sim, seller, &chair).await,
        0,
        "a listed piece leaves the inventory"
    );

    // Anyone can look at the stall and buy from it.
    let view = sim
        .command_resolved(
            buyer,
            "stall",
            json!([sub("view", vec![user_option("user", seller)])]),
            resolved(&[seller], &[]),
        )
        .await;
    assert!(
        !view.ephemeral && view.title().ends_with("'s stall"),
        "{}",
        view.title()
    );
    assert_eq!(size_of(&view), (1000, 600));
    let listing = view.options("sb:")[0].value.clone();
    assert_eq!(view.options("sb:").len(), 1);
    let sale = sim.pick(buyer, &view, "sb:", &listing).await;
    assert!(
        !sale.ephemeral
            && sale.content().contains(&format!("<@{buyer}> bought"))
            && sale.content().contains("200 shells"),
        "{}",
        sale.content()
    );
    assert!(
        sale.pings_only(seller),
        "the seller is told, nobody else: {:?}",
        sale.mentions
    );
    assert_eq!(shells(&sim, buyer).await, 800);
    assert_eq!(shells(&sim, seller).await, 190, "the seller keeps all but the 5% fee");
    assert_eq!(
        (owned(&sim, buyer, &chair).await, owned(&sim, seller, &chair).await),
        (1, 0)
    );
    // Somebody else with the same menu is too late; nothing is paid twice.
    fund(&sim, other, 500).await;
    let late = sim.pick(other, &view, "sb:", &listing).await;
    assert!(
        late.ephemeral && late.content().contains("already sold"),
        "{}",
        late.content()
    );
    assert_eq!((shells(&sim, other).await, shells(&sim, seller).await), (500, 190));

    // A listing can be taken back, and cannot be bought by the one who made it.
    let menu = sim
        .command(seller, "stall", json!([sub("sell", vec![int_option("price", 5_000)])]))
        .await;
    sim.pick(seller, &menu, "sl:5000", &lamp).await;
    let manage = sim.command(seller, "stall", json!([sub("manage", vec![])])).await;
    assert!(
        manage.ephemeral && manage.content().contains("Pick a listing"),
        "{}",
        manage.content()
    );
    let own_view = sim.command(seller, "stall", json!([sub("view", vec![])])).await;
    let own_listing = own_view.options("sb:")[0].value.clone();
    let own = sim.pick(seller, &own_view, "sb:", &own_listing).await;
    assert!(own.content().contains("your own listing"), "{}", own.content());
    // The refused purchase must not have eaten the listing.
    let manage = sim.command(seller, "stall", json!([sub("manage", vec![])])).await;
    assert_eq!(manage.options("su").len(), 1, "{}", manage.content());
    let id = manage.options("su")[0].value.clone();
    let back = sim.pick(seller, &manage, "su", &id).await;
    assert!(
        back.content().contains("back into your inventory"),
        "{}",
        back.content()
    );
    assert_eq!(owned(&sim, seller, &lamp).await, 1);
    let gone = sim.pick(seller, &manage, "su", &id).await;
    assert!(gone.content().contains("gone"), "{}", gone.content());
    // Too poor to pay: the piece stays on the stall.
    let menu = sim
        .command(seller, "stall", json!([sub("sell", vec![int_option("price", 5_000)])]))
        .await;
    sim.pick(seller, &menu, "sl:5000", &lamp).await;
    let view = sim
        .command_resolved(
            buyer,
            "stall",
            json!([sub("view", vec![user_option("user", seller)])]),
            resolved(&[seller], &[]),
        )
        .await;
    let dear = view.options("sb:")[0].value.clone();
    let poor = sim.pick(buyer, &view, "sb:", &dear).await;
    assert!(
        poor.ephemeral && poor.content().contains("It costs 5,000 shells; you have 800"),
        "{}",
        poor.content()
    );
    let still = sim.command(seller, "stall", json!([sub("manage", vec![])])).await;
    assert_eq!(still.options("su").len(), 1, "the unsold piece is still listed");
}

// /Plaza

#[tokio::test]
async fn the_plaza_takes_donations_and_managers_place_them() {
    let Some(mut sim) = Sim::new("the_plaza_takes_donations_and_managers_place_them").await else {
        return;
    };
    let (member, manager) = (unique(), unique());
    let piece = catalog_piece(7);
    let name = Content::get().item(&piece).expect("in the content").name();
    sim.app.db.grant_item(member, &piece, 1).await.unwrap();

    // Anyone can look: the server's own room, named after the server.
    let view = sim.command(member, "plaza", json!([sub("view", vec![])])).await;
    assert_eq!(view.title(), format!("{GUILD_NAME} plaza"));
    assert!(view.description().contains("0 items"), "{}", view.description());
    assert!(size_of(&view).0 > 100);

    // A donation moves one piece into the plaza's stock, and the channel hears about it.
    let nobody = sim.command(unique(), "plaza", json!([sub("donate", vec![])])).await;
    assert!(
        nobody.ephemeral && nobody.content().contains("inventory is empty"),
        "{}",
        nobody.content()
    );
    let menu = sim.command(member, "plaza", json!([sub("donate", vec![])])).await;
    assert!(menu.ephemeral && menu.options("pd").iter().any(|o| o.value == piece));
    let donated = sim.pick(member, &menu, "pd", &piece).await;
    assert!(donated.content().contains("Donated"), "{}", donated.content());
    let news = donated.followups.first().expect("the channel is told");
    assert!(
        !news.ephemeral && news.content().contains(&name) && news.content().contains("/plaza edit"),
        "{}",
        news.content()
    );
    assert_eq!(
        (owned(&sim, member, &piece).await, owned(&sim, sim.guild, &piece).await),
        (0, 1)
    );
    let again = sim.pick(member, &menu, "pd", &piece).await;
    assert!(
        again.ephemeral && again.content().contains("anymore"),
        "{}",
        again.content()
    );

    // Only managers arrange it.
    let refused = sim.command(member, "plaza", json!([sub("edit", vec![])])).await;
    assert!(
        refused.ephemeral && refused.content().contains("Only server managers"),
        "{}",
        refused.content()
    );
    sim.permissions = MANAGE_GUILD;
    let editor = sim.command(manager, "plaza", json!([sub("edit", vec![])])).await;
    sim.permissions = 0;
    assert!(editor.title().starts_with("Editing"), "{}", editor.title());
    assert!(
        editor.options("pe:s:").iter().any(|o| o.value == piece),
        "the donation is in the menu"
    );
    assert_eq!(editor.files.len(), 1);
    // A member cannot drive a manager's editor.
    let held = {
        sim.permissions = MANAGE_GUILD;
        let held = sim.pick(manager, &editor, "pe:s:", &piece).await;
        sim.permissions = 0;
        held
    };
    let sneaky = sim.click(member, &held, "Place").await;
    assert!(
        sneaky.content().contains("Only server managers"),
        "{}",
        sneaky.content()
    );
    sim.permissions = MANAGE_GUILD;
    let placed = sim.click(manager, &held, "Place").await;
    sim.permissions = 0;
    assert!(placed.description().contains("Placed"), "{}", placed.description());
    assert_eq!(sim.app.db.placements(sim.guild, 0).await.unwrap().len(), 1);
    let view = sim.command(member, "plaza", json!([sub("view", vec![])])).await;
    assert!(view.description().contains("1 item"), "{}", view.description());
}

// /Quests

#[tokio::test]
async fn quests_show_progress_and_pay_once() {
    let Some(mut sim) = Sim::new("quests_show_progress_and_pay_once").await else {
        return;
    };
    let user = unique();
    let day = today();
    let quests = pinchora_game::quests::for_day(user, day);
    let board = sim.command(user, "quests", json!([])).await;
    assert!(board.ephemeral && board.title() == "Today's quests");
    for quest in &quests {
        assert!(board.description().contains(quest.text), "{}", board.description());
    }
    assert!(
        board.description().contains("Finish all three"),
        "{}",
        board.description()
    );
    assert!(board.button("Claim rewards").disabled, "nothing is finished yet");
    // A click anyway (an old button): nothing to claim, nothing paid.
    let idle = sim.component(user, &format!("qc:{day}"), &[], &board).await;
    assert!(
        idle.description().contains("Nothing new to claim"),
        "{}",
        idle.description()
    );
    assert_eq!(shells(&sim, user).await, 0);

    // Two of the three done: they can be claimed, the all-clear pearls wait for the third.
    let app = sim.app.clone();
    let progress = |kind: pinchora_game::quests::QuestKind, times: i64| {
        let app = app.clone();
        async move {
            for _ in 0..times {
                app.quest(user, kind).await;
            }
        }
    };
    for quest in &quests[..2] {
        progress(quest.kind, quest.target).await;
    }
    let half = sim.command(user, "quests", json!([])).await;
    assert!(!half.button("Claim rewards").disabled);
    assert!(half.description().contains("🎁"), "{}", half.description());
    let claimed = sim.click(user, &half, "Claim rewards").await;
    let reward: i64 = quests[..2].iter().map(|q| q.reward).sum();
    assert!(
        claimed.description().contains(&format!("+{reward} shells"))
            && !claimed.description().contains("pearls for clearing"),
        "{}",
        claimed.description()
    );
    assert_eq!(shells(&sim, user).await, reward);
    assert!(claimed.description().contains("✅"), "{}", claimed.description());
    assert!(claimed.button("Claim rewards").disabled, "the claimed ones are done");
    let twice = sim.click(user, &half, "Claim rewards").await;
    assert!(
        twice.description().contains("Nothing new to claim"),
        "{}",
        twice.description()
    );
    assert_eq!(shells(&sim, user).await, reward, "a second click pays nothing");

    // The third finishes the day: its shells and the pearls come once.
    progress(quests[2].kind, quests[2].target).await;
    let pearls = sim.app.db.user(user).await.unwrap().pearls;
    let last = sim.command(user, "quests", json!([])).await;
    let all = sim.click(user, &last, "Claim rewards").await;
    assert!(
        all.description().contains(&format!("+{} shells", quests[2].reward))
            && all
                .description()
                .contains(&format!("+{} pearls", pinchora_game::quests::ALL_CLEAR_PEARLS)),
        "{}",
        all.description()
    );
    let user_row = sim.app.db.user(user).await.unwrap();
    assert_eq!(user_row.credits, reward + quests[2].reward);
    assert_eq!(user_row.pearls, pearls + pinchora_game::quests::ALL_CLEAR_PEARLS);
    let done = sim.click(user, &last, "Claim rewards").await;
    assert!(
        done.description().contains("Nothing new to claim"),
        "{}",
        done.description()
    );
    assert_eq!(sim.app.db.user(user).await.unwrap().pearls, user_row.pearls);
}

// /House

#[tokio::test]
async fn house_view_visit_like_and_lock() {
    let Some(mut sim) = Sim::new("house_view_visit_like_and_lock").await else {
        return;
    };
    let (host, guest, other, bot) = (unique(), unique(), unique(), unique());

    // A first look makes the house: one room of the first tier, nothing placed, the starter kit kept.
    let view = sim.command(host, "house", json!([sub("view", vec![])])).await;
    let tier = house::tier(1);
    assert_eq!(view.title(), format!("Sim's {}", tier.name));
    assert!(
        view.description().contains("Room 1/1") && view.description().contains("0 items"),
        "{}",
        view.description()
    );
    assert!(
        !view.ephemeral && view.components.is_empty(),
        "one room, no guests: no buttons"
    );
    assert!(size_of(&view).0 > 200);
    assert!(sim.app.db.inventory(host).await.unwrap().len() >= 3, "the starter kit");

    // A visit pays the guest once a day and lets them sign the guestbook.
    let visit = |target| json!([sub("visit", vec![user_option("user", target)])]);
    let first = sim
        .command_resolved(guest, "house", visit(host), resolved(&[host], &[]))
        .await;
    assert!(
        !first.ephemeral && first.title().ends_with(tier.name),
        "{}",
        first.title()
    );
    assert_eq!(
        first.embeds[0].footer.as_ref().map(|f| f.text.as_str()),
        Some("+25 shells for visiting today")
    );
    first.button("Sign guestbook");
    assert_eq!(shells(&sim, guest).await, 25);
    let second = sim
        .command_resolved(guest, "house", visit(host), resolved(&[host], &[]))
        .await;
    assert!(second.embeds[0].footer.is_none(), "paid once a day");
    assert_eq!(shells(&sim, guest).await, 25);
    let own = sim
        .command_resolved(host, "house", visit(host), resolved(&[host], &[]))
        .await;
    assert!(!own.has_button("Sign guestbook"), "nobody signs their own book");
    assert_eq!(shells(&sim, host).await, 0, "visiting yourself pays nothing");
    let robot = sim
        .command_resolved(guest, "house", visit(bot), bot_resolved(bot))
        .await;
    assert!(
        robot.content().contains("Bots don't have houses"),
        "{}",
        robot.content()
    );

    // A like is public and counts once a day; the next day it counts again.
    let like = |target| json!([sub("like", vec![user_option("user", target)])]);
    let liked = sim
        .command_resolved(guest, "house", like(host), resolved(&[host], &[]))
        .await;
    assert!(
        !liked.ephemeral && liked.content().contains("liked") && liked.content().contains("**1** like now"),
        "{}",
        liked.content()
    );
    let twice = sim
        .command_resolved(guest, "house", like(host), resolved(&[host], &[]))
        .await;
    assert!(
        twice.ephemeral && twice.content().contains("already liked"),
        "{}",
        twice.content()
    );
    let by_other = sim
        .command_resolved(other, "house", like(host), resolved(&[host], &[]))
        .await;
    assert!(by_other.content().contains("**2** likes now"), "{}", by_other.content());
    let myself = sim
        .command_resolved(host, "house", like(host), resolved(&[host], &[]))
        .await;
    assert!(
        myself.content().contains("someone else's house"),
        "{}",
        myself.content()
    );
    let shown = sim.command(host, "house", json!([sub("view", vec![])])).await;
    assert!(shown.description().contains("💖 2"), "{}", shown.description());
    sim.app.shift_clock(chrono::Duration::hours(25));
    let next_day = sim
        .command_resolved(guest, "house", like(host), resolved(&[host], &[]))
        .await;
    assert!(next_day.content().contains("**3** likes now"), "{}", next_day.content());

    // A locked house turns visitors away, and the owner still walks in.
    let lock = |locked| json!([sub("lock", vec![bool_option("locked", locked)])]);
    let locked = sim.command(host, "house", lock(true)).await;
    assert!(
        locked.ephemeral && locked.content().contains("is locked"),
        "{}",
        locked.content()
    );
    let turned_away = sim
        .command_resolved(other, "house", visit(host), resolved(&[host], &[]))
        .await;
    assert!(
        turned_away.ephemeral && turned_away.content().contains("has locked their house"),
        "{}",
        turned_away.content()
    );
    let own = sim
        .command_resolved(host, "house", visit(host), resolved(&[host], &[]))
        .await;
    assert!(!own.ephemeral, "the owner is let in");
    let opened = sim.command(host, "house", lock(false)).await;
    assert!(opened.content().contains("open for visitors"), "{}", opened.content());
    let let_in = sim
        .command_resolved(other, "house", visit(host), resolved(&[host], &[]))
        .await;
    assert!(
        !let_in.ephemeral && let_in.title().ends_with(tier.name),
        "{}",
        let_in.title()
    );
}

#[tokio::test]
async fn house_decorate_and_upgrade_charge_what_they_show() {
    let Some(mut sim) = Sim::new("house_decorate_and_upgrade_charge_what_they_show").await else {
        return;
    };
    let (user, broke) = (unique(), unique());
    fund(&sim, user, 20_000).await;
    let content = Content::get();

    // Decorating: a menu of wallpapers and one of floors; a new pattern is bought on first use.
    let panel = sim.command(user, "house", json!([sub("decorate", vec![])])).await;
    assert!(
        panel.ephemeral && panel.title() == "Decorating room 1",
        "{}",
        panel.title()
    );
    assert!(size_of(&panel).0 > 200);
    let current = panel.options("dw:").iter().find(|o| o.default).map(|o| o.label.clone());
    let wallpaper = content
        .surfaces
        .wallpapers
        .iter()
        .take(25)
        .filter(|w| w.currency == pinchora_game::Currency::Shells && w.price > 0 && Some(&w.name) != current.as_ref())
        .min_by_key(|w| w.price)
        .expect("a wallpaper for shells");
    let before = shells(&sim, user).await;
    let chosen = sim.pick(user, &panel, "dw:", &wallpaper.id).await;
    assert!(
        chosen.description().contains(&format!("Bought **{}**", wallpaper.name))
            && chosen
                .description()
                .contains(&format!("Wallpaper: **{}**", wallpaper.name)),
        "{}",
        chosen.description()
    );
    assert_eq!(before - shells(&sim, user).await, wallpaper.price);
    // Going back to the old pattern and forth again costs nothing more.
    let floor = content
        .surfaces
        .floors
        .iter()
        .take(25)
        .filter(|f| f.currency == pinchora_game::Currency::Shells && f.price > 0)
        .min_by_key(|f| f.price)
        .expect("a floor for shells");
    let after_floor = sim.pick(user, &chosen, "df:", &floor.id).await;
    assert!(
        after_floor
            .description()
            .contains(&format!("Floor: **{}**", floor.name)),
        "{}",
        after_floor.description()
    );
    let spent = shells(&sim, user).await;
    let again = sim.pick(user, &after_floor, "dw:", &wallpaper.id).await;
    assert!(!again.description().contains("Bought"), "{}", again.description());
    assert_eq!(shells(&sim, user).await, spent, "an owned pattern is free to switch to");
    // Without the shells the pattern is not applied.
    let poor_panel = sim.command(broke, "house", json!([sub("decorate", vec![])])).await;
    let refused = sim.pick(broke, &poor_panel, "dw:", &wallpaper.id).await;
    assert!(
        refused.ephemeral && refused.content().contains("costs"),
        "{}",
        refused.content()
    );
    let unchanged = sim.command(broke, "house", json!([sub("decorate", vec![])])).await;
    assert!(
        !unchanged
            .description()
            .contains(&format!("Wallpaper: **{}**", wallpaper.name))
    );
    let junk = sim.component(user, "dw:0:0", &["no_such_pattern"], &panel).await;
    assert!(junk.content().contains("Pick a pattern"), "{}", junk.content());

    // Upgrading: the offer names the price, the button waits for the shells, a click pays once.
    let next = house::tier(2);
    let offer = sim.command(broke, "house", json!([sub("upgrade", vec![])])).await;
    assert!(
        offer.ephemeral && offer.title() == format!("Upgrade to the {}", next.name),
        "{}",
        offer.title()
    );
    assert!(offer.description().contains("5,000"), "{}", offer.description());
    let label = format!("Upgrade for {} shells", thousands(next.price));
    assert!(offer.button(&label).disabled, "5,000 shells are not in this wallet");
    let forged = sim.component(broke, "hu:1", &[], &offer).await;
    assert!(
        forged.content().contains("You need 5,000 shells but have 0"),
        "{}",
        forged.content()
    );
    let offer = sim.command(user, "house", json!([sub("upgrade", vec![])])).await;
    assert!(!offer.button(&label).disabled);
    let before = shells(&sim, user).await;
    let upgraded = sim.click(user, &offer, &label).await;
    assert!(
        upgraded
            .content()
            .contains(&format!("Welcome to your **{}**", next.name)),
        "{}",
        upgraded.content()
    );
    assert_eq!(before - shells(&sim, user).await, next.price);
    let old_button = sim.component(user, "hu:1", &[], &offer).await;
    assert!(
        old_button.content().contains("already changed"),
        "{}",
        old_button.content()
    );
    assert_eq!(
        before - shells(&sim, user).await,
        next.price,
        "an old button does not charge again"
    );
    let view = sim.command(user, "house", json!([sub("view", vec![])])).await;
    assert_eq!(view.title(), format!("Sim's {}", next.name));
    assert!(view.description().contains("Room 1/2"), "{}", view.description());
    assert!(
        view.has_button("Room ▶") && view.button("◀ Room").disabled,
        "two rooms now"
    );
    let next_offer = sim.command(user, "house", json!([sub("upgrade", vec![])])).await;
    assert_eq!(next_offer.title(), format!("Upgrade to the {}", house::tier(3).name));
}

// /Config

#[tokio::test]
async fn config_sets_the_welcome_card_and_leveling_and_shows_them() {
    let Some(mut sim) = Sim::new("config_sets_the_welcome_card_and_leveling_and_shows_them").await else {
        return;
    };
    let (manager, member) = (unique(), unique());
    let channel = unique();
    let config = |sub_command: Value| json!([sub_command]);

    // Members cannot change anything, whatever the command's menu in Discord says.
    let refused = sim
        .command(
            member,
            "config",
            config(sub("welcome", vec![bool_option("enabled", true)])),
        )
        .await;
    assert!(
        refused.ephemeral && refused.content().contains("Manage Server"),
        "{}",
        refused.content()
    );
    assert!(!sim.app.db.guild_settings(sim.guild).await.unwrap().welcome_enabled);

    sim.permissions = MANAGE_GUILD;
    let welcome = sim
        .command_resolved(
            manager,
            "config",
            config(sub(
                "welcome",
                vec![
                    bool_option("enabled", true),
                    channel_option("channel", channel),
                    text_option("message", "Ahoy {user}, welcome to {server}!"),
                ],
            )),
            resolved_channels(&[channel]),
        )
        .await;
    assert!(
        welcome.ephemeral && welcome.title() == "Server settings",
        "{}",
        welcome.title()
    );
    let field = welcome.field("Welcome cards");
    assert!(
        field.contains("**on**")
            && field.contains(&format!("<#{channel}>"))
            && field.contains("Ahoy {user}, welcome to {server}!"),
        "{field}"
    );
    let stored = sim.app.db.guild_settings(sim.guild).await.unwrap();
    assert!(stored.welcome_enabled && stored.welcome_channel_id == Some(channel as i64));

    let leveling = sim
        .command(
            manager,
            "config",
            config(sub(
                "leveling",
                vec![
                    bool_option("enabled", false),
                    text_option("message", "{user} is level {level}!"),
                    text_option("mode", "dm"),
                ],
            )),
        )
        .await;
    let field = leveling.field("Leveling");
    assert!(
        field.contains("**off**") && field.contains("direct message") && field.contains("{user} is level {level}!"),
        "{field}"
    );
    assert_eq!(
        leveling.field("Welcome cards"),
        welcome.field("Welcome cards"),
        "one command leaves the other's settings alone"
    );

    // The overview shows every part, including the ones other commands keep.
    let shown = sim.command(manager, "config", config(sub("show", vec![]))).await;
    let names: Vec<&str> = shown.embeds[0].fields.iter().map(|f| f.name.as_str()).collect();
    for name in [
        "Welcome cards",
        "Leveling",
        "Server points",
        "Goodbye",
        "Server log",
        "Suggestions",
        "Critter drops",
        "Party games",
    ] {
        assert!(names.contains(&name), "{name} in {names:?}");
    }
    assert_eq!(shown.field("Leveling"), leveling.field("Leveling"));

    // The welcome card: a preview with the design saved, and a colour that is not one is refused.
    let card = sim
        .command(
            manager,
            "config",
            config(sub(
                "welcome-card",
                vec![
                    text_option("theme", "sunset"),
                    text_option("title", "HELLO"),
                    text_option("color", "#00ffcc"),
                ],
            )),
        )
        .await;
    assert!(
        card.ephemeral && card.title() == "Welcome card preview",
        "{}",
        card.title()
    );
    assert_eq!(size_of(&card).0, 1100);
    let stored = sim.app.db.guild_settings(sim.guild).await.unwrap();
    assert_eq!(
        (
            stored.welcome_theme.as_deref(),
            stored.welcome_title.as_deref(),
            stored.welcome_accent
        ),
        (Some("sunset"), Some("HELLO"), Some(0x00ffcc))
    );
    let bad = sim
        .command(
            manager,
            "config",
            config(sub("welcome-card", vec![text_option("color", "#12345g")])),
        )
        .await;
    assert!(bad.ephemeral && bad.files.is_empty(), "{}", bad.content());
    assert_eq!(
        sim.app.db.guild_settings(sim.guild).await.unwrap().welcome_accent,
        Some(0x00ffcc),
        "a refused colour changes nothing"
    );
    let plain = sim
        .command(manager, "config", config(sub("welcome-card", vec![])))
        .await;
    assert_eq!(size_of(&plain).0, 1100, "the preview of what is saved");
    sim.permissions = 0;
}

// /Duel tictactoe

#[tokio::test]
async fn tic_tac_toe_enforces_turns_and_a_full_board_is_a_draw() {
    let Some(mut sim) = Sim::new("tic_tac_toe_enforces_turns_and_a_full_board_is_a_draw").await else {
        return;
    };
    let (x, o, stranger) = (unique(), unique(), unique());
    let mut game = duel(&mut sim, x, o, "tictactoe", vec![]).await;
    assert!(!game.ephemeral, "a duel is played in the channel");
    assert_eq!(game.controls().count(), 9, "nine squares");
    let cell = |game: &Reply, n: usize| match game.controls().nth(n) {
        Some(Component::Button(b)) => b.custom_id.clone().expect("custom id"),
        other => panic!("cell {n} is {other:?}"),
    };

    // O cannot go first, a stranger cannot play, and a taken square cannot be taken again.
    let early = sim.component(o, &cell(&game, 4), &[], &game).await;
    assert!(
        early.ephemeral && early.content().contains("Not your turn"),
        "{}",
        early.content()
    );
    let outsider = sim.component(stranger, &cell(&game, 4), &[], &game).await;
    assert!(
        outsider.ephemeral && outsider.content().contains("not in this game"),
        "{}",
        outsider.content()
    );
    game = sim.component(x, &cell(&game, 0), &[], &game).await;
    let taken = sim.component(o, &cell(&game, 0), &[], &game).await;
    assert!(
        taken.ephemeral && taken.content().contains("taken"),
        "{}",
        taken.content()
    );

    // X O X / X O O / O X X: nobody lines up three.
    for (player, n) in [(o, 1), (x, 2), (o, 4), (x, 3), (o, 5), (x, 7), (o, 6), (x, 8)] {
        game = sim.component(player, &cell(&game, n), &[], &game).await;
    }
    assert!(game.content().contains("draw"), "{}", game.content());
    assert_eq!(
        (shells(&sim, x).await, shells(&sim, o).await),
        (0, 0),
        "a draw pays nothing"
    );
    assert_eq!(
        (
            record_of(&sim, x, "tictactoe").await,
            record_of(&sim, o, "tictactoe").await
        ),
        ((1, 0), (1, 0))
    );
    // A click on the finished board changes nothing and counts nothing again.
    let late = sim.component(x, &cell(&game, 0), &[], &game).await;
    assert!(late.ephemeral && late.content().contains("taken"), "{}", late.content());
    assert_eq!(record_of(&sim, x, "tictactoe").await, (1, 0));
}
