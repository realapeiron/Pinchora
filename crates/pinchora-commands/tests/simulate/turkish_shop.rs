//! Turkish for the shop group, played through the real handlers: the market, catalog and inventory, gifts, stalls
//! and the plaza, quests, server points and the role store. The pictures land in `target/sim/<test name>/`.

use pinchora_core::{i18n::Lang, items::soulbound};
use pinchora_game::{Availability, Currency, quests::QuestKind};

use super::*;

/// The id of a piece of furniture sold in the catalog for shells, by position.
fn catalog_piece(n: usize) -> String {
    Content::get()
        .items()
        .filter(|i| i.base.availability == Availability::Permanent && i.base.currency == Currency::Shells)
        .nth(n)
        .map(|i| i.id())
        .expect("a catalog piece")
}

fn keep_png(sim: &Sim, reply: &Reply, name: &str) {
    let (_, png) = reply.files.first().expect("the reply carried a picture");
    std::fs::write(sim.dir.join(format!("{name}.png")), png).expect("save the picture");
}

/// The footer of a message's first embed.
fn footer(reply: &Reply) -> &str {
    reply
        .embeds
        .first()
        .and_then(|e| e.footer.as_ref())
        .map_or("", |f| f.text.as_str())
}

#[tokio::test]
async fn turkish_market_catalog_and_inventory() {
    let Some(mut sim) = Sim::new("turkish_market_catalog_and_inventory").await else {
        return;
    };
    let (user, broke) = (unique(), unique());
    fund(&sim, user, 200_000).await;
    let content = Content::get();
    sim.locale = "tr";

    // The hourly market: Turkish chrome, the Turkish name of every piece, the price taken once.
    let market = sim.command(user, "shop", json!([sub("market", vec![])])).await;
    assert_eq!(market.title(), "Saatlik pazar");
    assert!(
        market.description().contains("Her saat yenilenen"),
        "{}",
        market.description()
    );
    assert_eq!(market.files[0].0, "market.png");
    keep_png(&sim, &market, "market-tr");
    let offers: Vec<_> = market.options("mk:").to_vec();
    assert!(offers.len() <= 8);
    if let Some(first) = offers.first() {
        let item = content.item(&first.value).expect("in the content");
        assert_eq!(first.label, item.name_in(Lang::Tr));
        let description = first.description.as_deref().unwrap_or_default();
        assert!(
            description.contains("kabuk") && description.contains("kaldı"),
            "{description}"
        );
        let before = shells(&sim, user).await;
        let reply = sim.pick(user, &market, "mk:", &first.value).await;
        assert!(reply.ephemeral);
        if reply.content().starts_with("🛍️ ") {
            assert!(
                reply.content().contains("satın alındı") && reply.content().contains("kabuk"),
                "{}",
                reply.content()
            );
            assert!(shells(&sim, user).await < before);
        } else {
            assert!(reply.content().contains("tükendi"), "{}", reply.content());
        }
        let poor = sim.pick(broke, &market, "mk:", &first.value).await;
        assert!(
            poor.content().contains("sende 0 var") || poor.content().contains("tükendi"),
            "{}",
            poor.content()
        );
    }
    let stale = sim.component(user, "mk:1", &[&catalog_piece(0)], &market).await;
    assert!(stale.content().contains("Bu pazar kapandı"), "{}", stale.content());

    // The catalog, in the category with the longest Turkish name.
    let catalog = sim
        .command(
            user,
            "shop",
            json!([sub("catalog", vec![text_option("category", "pet_items")])]),
        )
        .await;
    assert_eq!(catalog.title(), "Katalog: Evcil hayvan eşyaları");
    assert!(
        catalog.description().contains("`/inventory`"),
        "{}",
        catalog.description()
    );
    assert!(catalog.button("◀ Sayfa").disabled);
    keep_png(&sim, &catalog, "catalog-pet-items-tr");
    let cheap = catalog
        .options("cb:pet_items")
        .iter()
        .find(|o| !o.description.as_deref().unwrap_or_default().contains("inci"))
        .expect("a pet item for shells")
        .clone();
    assert!(
        cheap.description.as_deref().unwrap_or_default().contains(" kabuk · "),
        "{:?}",
        cheap.description
    );
    let price = content.item(&cheap.value).expect("in the content").base.price;
    let before = shells(&sim, user).await;
    let bought = sim.pick(user, &catalog, "cb:pet_items", &cheap.value).await;
    assert!(
        bought.ephemeral
            && bought.content().starts_with("🛍️ ")
            && bought.content().contains("satın alındı")
            && bought.content().contains("/house edit"),
        "{}",
        bought.content()
    );
    assert_eq!(before - shells(&sim, user).await, price);
    let too_dear = sim.pick(broke, &catalog, "cb:pet_items", &cheap.value).await;
    assert!(too_dear.content().contains("sende 0 var"), "{}", too_dear.content());
    let unknown = sim.component(user, "cb:pet_items", &["no_such_piece"], &catalog).await;
    assert_eq!(unknown.content(), "Satın almak için bir ürün seç.");
    let rotating = content
        .items()
        .find(|i| i.base.availability != Availability::Permanent)
        .map(|i| i.id())
        .expect("a piece that is not in the catalog");
    let refused = sim.component(user, "cb:pet_items", &[&rotating], &catalog).await;
    assert!(
        refused.content().contains("katalogda satılmıyor"),
        "{}",
        refused.content()
    );

    let doors = sim
        .command(
            user,
            "shop",
            json!([sub("catalog", vec![text_option("category", "windows_doors")])]),
        )
        .await;
    assert_eq!(doors.title(), "Katalog: Pencere ve kapılar");
    keep_png(&sim, &doors, "catalog-windows-doors-tr");

    // A page with a pearl price, so the card's İNCİ line is looked at too.
    let pearl = content
        .items()
        .find(|i| i.base.availability == Availability::Permanent && i.base.currency == Currency::Pearls)
        .expect("a catalog piece for pearls");
    let category = pearl.base.category;
    let mut aisle: Vec<_> = content
        .items()
        .filter(|i| i.base.category == category && i.base.availability == Availability::Permanent)
        .collect();
    aisle.sort_by_key(|i| (i.base.currency == Currency::Pearls, i.base.price, i.id()));
    let page = aisle.iter().position(|i| i.id() == pearl.id()).expect("in its aisle") / 8;
    let shown = sim
        .component(user, &format!("cp:{}:{page}", category.key()), &[], &catalog)
        .await;
    assert!(shown.title().starts_with("Katalog: "), "{}", shown.title());
    keep_png(&sim, &shown, "catalog-pearls-tr");
    let dear = shown
        .options(&format!("cb:{}", category.key()))
        .iter()
        .find(|o| o.value == pearl.id())
        .expect("the pearl piece is on its page")
        .clone();
    assert!(
        dear.description.as_deref().unwrap_or_default().contains(" inci · "),
        "{:?}",
        dear.description
    );
    let poor = sim
        .pick(broke, &shown, &format!("cb:{}", category.key()), &pearl.id())
        .await;
    assert!(
        poor.content().contains("inci gerekiyor; sende 0 var"),
        "{}",
        poor.content()
    );

    // The inventory: what was bought, a soulbound piece with its badge, a second page once it is full.
    sim.app
        .db
        .grant_item(user, &soulbound(&catalog_piece(1)), 1)
        .await
        .unwrap();
    let inventory = sim.command(user, "inventory", json!([])).await;
    assert!(
        inventory.ephemeral && inventory.title() == "Envanterin",
        "{}",
        inventory.title()
    );
    assert!(inventory.description().contains("`/house edit`"));
    keep_png(&sim, &inventory, "inventory-tr");
    for n in 0..12 {
        sim.app.db.grant_item(user, &catalog_piece(40 + n), 1).await.unwrap();
    }
    let full = sim.command(user, "inventory", json!([])).await;
    assert!(full.button("◀ Sayfa").disabled && !full.button("Sayfa ▶").disabled);
    let second = sim.click(user, &full, "Sayfa ▶").await;
    assert!(!second.button("◀ Sayfa").disabled && second.button("Sayfa ▶").disabled);

    // The same commands in English stay as they were.
    sim.locale = "en-US";
    let market = sim.command(user, "shop", json!([sub("market", vec![])])).await;
    assert_eq!(market.title(), "Hourly market");
    let catalog = sim
        .command(
            user,
            "shop",
            json!([sub("catalog", vec![text_option("category", "beds")])]),
        )
        .await;
    assert_eq!(catalog.title(), "Beds catalog");
    assert!(catalog.has_button("◀ Page") && catalog.has_button("Page ▶"));
    let inventory = sim.command(user, "inventory", json!([])).await;
    assert_eq!(inventory.title(), "Your inventory");
}

#[tokio::test]
async fn turkish_gifts_stalls_and_the_plaza() {
    let Some(mut sim) = Sim::new("turkish_gifts_stalls_and_the_plaza").await else {
        return;
    };
    let (giver, friend, empty_handed, buyer, member, manager) =
        (unique(), unique(), unique(), unique(), unique(), unique());
    let (sofa, chair, lamp, donation) = (catalog_piece(3), catalog_piece(5), catalog_piece(9), catalog_piece(7));
    let name_of = |id: &str| Content::get().item(id).expect("in the content").name_in(Lang::Tr);
    let db = sim.app.db.clone();
    for (user, piece) in [(giver, &sofa), (giver, &chair), (giver, &lamp), (member, &donation)] {
        db.grant_item(user, piece, 1).await.unwrap();
    }
    // A soulbound piece that may not leave its owner.
    let bound = soulbound(&catalog_piece(2));
    db.grant_item(giver, &bound, 1).await.unwrap();
    fund(&sim, buyer, 1_000).await;
    sim.locale = "tr";

    // /gift: nothing to give, not to yourself, the menu in Turkish, the channel hears of it.
    let gift = |to| json!([user_option("user", to)]);
    let nothing = sim
        .command_resolved(empty_handed, "gift", gift(friend), resolved(&[friend], &[]))
        .await;
    assert!(
        nothing.ephemeral && nothing.content().starts_with("Envanterin boş."),
        "{}",
        nothing.content()
    );
    let myself = sim
        .command_resolved(giver, "gift", gift(giver), resolved(&[giver], &[]))
        .await;
    assert_eq!(myself.content(), "Hediyeler arkadaşlar için!");
    let menu = sim
        .command_resolved(giver, "gift", gift(friend), resolved(&[friend], &[]))
        .await;
    assert!(
        menu.ephemeral
            && menu
                .content()
                .contains(&format!("<@{friend}> kullanıcısına ne göndermek")),
        "{}",
        menu.content()
    );
    assert!(
        menu.options("gf:")
            .iter()
            .any(|o| o.value == sofa && o.label == name_of(&sofa))
    );
    assert!(
        !menu.options("gf:").iter().any(|o| o.value == bound),
        "the soulbound piece is left out"
    );
    assert!(
        menu.options("gf:")[0]
            .description
            .as_deref()
            .is_some_and(|d| d.starts_with("Sende ")),
        "{:?}",
        menu.options("gf:")[0].description
    );
    let forged = sim.component(giver, &format!("gf:{friend}"), &[&bound], &menu).await;
    assert!(
        forged.content().contains("başkasına verilemez") && forged.content().contains("ruh bağlıdır"),
        "{}",
        forged.content()
    );
    let sent = sim.pick(giver, &menu, "gf:", &sofa).await;
    assert_eq!(sent.content(), "🎁 Gönderildi!");
    let news = sent.followups.first().expect("the channel is told");
    assert!(
        !news.ephemeral
            && news.content().contains(&name_of(&sofa))
            && news.content().contains(&format!("<@{friend}> kullanıcısına")),
        "{}",
        news.content()
    );
    assert!(news.pings_only(friend));
    let again = sim.pick(giver, &menu, "gf:", &sofa).await;
    assert!(again.content().contains("Artık sende"), "{}", again.content());

    // /stall: an empty one, a listing with the fee spelled out, the card, a purchase, the refusals.
    let empty = sim.command(giver, "stall", json!([sub("view", vec![])])).await;
    assert_eq!(empty.description(), "Şu an satılık bir şey yok.");
    let nothing = sim.command(empty_handed, "stall", json!([sub("manage", vec![])])).await;
    assert!(nothing.content().starts_with("Tezgâhın boş."), "{}", nothing.content());
    let sell = |price: i64| json!([sub("sell", vec![int_option("price", price)])]);
    let menu = sim.command(giver, "stall", sell(200)).await;
    assert!(
        menu.ephemeral
            && menu.content().contains("**200** kabuğa")
            && menu.content().contains("%5 komisyondan sonra eline 190 kabuk"),
        "{}",
        menu.content()
    );
    let refused = sim.component(giver, "sl:200", &[&bound], &menu).await;
    assert!(
        refused.content().contains("tezgâha konamaz") && refused.content().contains("ruh bağlıdır"),
        "{}",
        refused.content()
    );
    let listed = sim.pick(giver, &menu, "sl:", &chair).await;
    assert!(
        listed.content().contains("tezgâhında 200 kabuğa satışta"),
        "{}",
        listed.content()
    );
    let view = sim
        .command_resolved(
            buyer,
            "stall",
            json!([sub("view", vec![user_option("user", giver)])]),
            resolved(&[giver], &[]),
        )
        .await;
    assert!(
        !view.ephemeral && view.title().starts_with("Tezgâh: "),
        "{}",
        view.title()
    );
    assert!(
        view.description().contains("Satın almak için"),
        "{}",
        view.description()
    );
    keep_png(&sim, &view, "stall-tr");
    let listing = view.options("sb:")[0].value.clone();
    assert_eq!(view.options("sb:")[0].description.as_deref(), Some("200 kabuk"));
    let sale = sim.pick(buyer, &view, "sb:", &listing).await;
    assert!(
        !sale.ephemeral
            && sale
                .content()
                .contains(&format!("<@{buyer}>, <@{giver}> kullanıcısının tezgâhından"))
            && sale.content().contains("satın aldı: 200 kabuk")
            && sale.content().contains("Geriye 800 kabuk kaldı"),
        "{}",
        sale.content()
    );
    assert!(sale.pings_only(giver));
    let late = sim.pick(member, &view, "sb:", &listing).await;
    assert!(late.content().contains("çoktan satıldı"), "{}", late.content());
    // A dear listing: its seller cannot buy it, the buyer cannot afford it, it can be taken back once.
    let menu = sim.command(giver, "stall", sell(5_000)).await;
    sim.pick(giver, &menu, "sl:5000", &lamp).await;
    let own_view = sim.command(giver, "stall", json!([sub("view", vec![])])).await;
    let dear = own_view.options("sb:")[0].value.clone();
    let own = sim.pick(giver, &own_view, "sb:", &dear).await;
    assert!(own.content().contains("kendi ilanın"), "{}", own.content());
    let poor = sim.pick(buyer, &own_view, "sb:", &dear).await;
    assert_eq!(poor.content(), "Fiyatı 5,000 kabuk; sende 800 var.");
    let manage = sim.command(giver, "stall", json!([sub("manage", vec![])])).await;
    assert!(
        manage.content().starts_with("Envanterine geri almak"),
        "{}",
        manage.content()
    );
    assert_eq!(
        manage.options("su")[0].description.as_deref(),
        Some("5,000 kabuğa satışta")
    );
    let id = manage.options("su")[0].value.clone();
    let back = sim.pick(giver, &manage, "su", &id).await;
    assert!(back.content().contains("envanterine geri alındı"), "{}", back.content());
    let gone = sim.pick(giver, &manage, "su", &id).await;
    assert_eq!(gone.content(), "O ilan artık yok.");

    // /plaza: the donation menu, the announcement, the managers-only editor.
    let view = sim.command(member, "plaza", json!([sub("view", vec![])])).await;
    assert_eq!(view.title(), format!("{GUILD_NAME} meydanı"));
    let nobody = sim.command(unique(), "plaza", json!([sub("donate", vec![])])).await;
    assert_eq!(nobody.content(), "Envanterin boş.");
    let menu = sim.command(member, "plaza", json!([sub("donate", vec![])])).await;
    assert_eq!(menu.content(), "Meydana ne bağışlamak istiyorsun?");
    let donated = sim.pick(member, &menu, "pd", &donation).await;
    assert_eq!(donated.content(), "💝 Bağışlandı!");
    let news = donated.followups.first().expect("the channel is told");
    assert!(
        !news.ephemeral
            && news
                .content()
                .contains(&format!("<@{member}>, meydana **{}** bağışladı", name_of(&donation)))
            && news.content().contains("`/plaza edit`"),
        "{}",
        news.content()
    );
    let again = sim.pick(member, &menu, "pd", &donation).await;
    assert!(again.content().contains("Artık sende"), "{}", again.content());
    let forged = sim.component(giver, "pd", &[&bound], &menu).await;
    assert!(
        forged.content().contains("bağışlanamaz") && forged.content().contains("ruh bağlıdır"),
        "{}",
        forged.content()
    );
    let refused = sim.command(member, "plaza", json!([sub("edit", vec![])])).await;
    assert_eq!(
        refused.content(),
        "Meydanı yalnızca sunucu yöneticileri düzenleyebilir."
    );
    sim.permissions = MANAGE_GUILD;
    let editor = sim.command(manager, "plaza", json!([sub("edit", vec![])])).await;
    sim.permissions = 0;
    assert!(
        editor
            .description()
            .contains("Meydan düzenleniyor. Menüde bağışlanan mobilyalar var."),
        "{}",
        editor.description()
    );

    // The same, in English.
    sim.locale = "en-US";
    let myself = sim
        .command_resolved(giver, "gift", gift(giver), resolved(&[giver], &[]))
        .await;
    assert!(
        myself.content().contains("Gifts are for friends"),
        "{}",
        myself.content()
    );
    let nobody = sim.command(unique(), "plaza", json!([sub("donate", vec![])])).await;
    assert!(nobody.content().contains("inventory is empty"), "{}", nobody.content());
    let view = sim.command(buyer, "stall", json!([sub("view", vec![])])).await;
    assert!(view.title().ends_with("'s stall"), "{}", view.title());
}

#[tokio::test]
async fn turkish_quests_points_and_the_store() {
    let Some(mut sim) = Sim::new("turkish_quests_points_and_the_store").await else {
        return;
    };
    let (user, admin, buyer) = (unique(), unique(), unique());
    sim.locale = "tr";

    // /quests: the Turkish board, the claim button, the footer countdown, the claims.
    let day = today();
    let quests = pinchora_game::quests::for_day(user, day);
    let board = sim.command(user, "quests", json!([])).await;
    assert!(board.ephemeral && board.title() == "Bugünün görevleri");
    for quest in &quests {
        assert!(
            board.description().contains(quest.text_in(Lang::Tr)),
            "{}",
            board.description()
        );
    }
    assert!(board.description().contains("kabuk"), "{}", board.description());
    assert!(
        board.description().contains("Üçünü de bitirirsen"),
        "{}",
        board.description()
    );
    assert!(
        footer(&board).starts_with("Yeni görevlere ") && footer(&board).contains("dk"),
        "{}",
        footer(&board)
    );
    assert!(board.button("Ödülleri al").disabled);
    let idle = sim.component(user, &format!("qc:{day}"), &[], &board).await;
    assert!(
        idle.description().contains("Alınacak yeni bir şey yok"),
        "{}",
        idle.description()
    );
    let stale = sim.component(user, &format!("qc:{}", day - 1), &[], &board).await;
    assert!(
        stale.description().contains("eski bir güne ait"),
        "{}",
        stale.description()
    );
    let app = sim.app.clone();
    let progress = |kind: QuestKind, times: i64| {
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
    let claimed = sim.click(user, &half, "Ödülleri al").await;
    let reward: i64 = quests[..2].iter().map(|q| q.reward).sum();
    assert!(
        claimed.description().contains(&format!("🎉 +{reward} kabuk")),
        "{}",
        claimed.description()
    );
    progress(quests[2].kind, quests[2].target).await;
    let last = sim.command(user, "quests", json!([])).await;
    let all = sim.click(user, &last, "Ödülleri al").await;
    assert!(
        all.description().contains("inci, günü tamamladığın için!"),
        "{}",
        all.description()
    );
    assert_eq!(shells(&sim, user).await, reward + quests[2].reward);

    // /points: a currency whose name starts with a dotted i, so its capital has to be İ.
    sim.own_server(admin);
    sim.app
        .db
        .update_points(sim.guild, Some(true), Some("iyilik"), Some("🪙"), Some(2), Some(4))
        .await
        .unwrap();
    let give = |user: u64, amount: i64| {
        json!([sub(
            "give",
            vec![user_option("user", user), int_option("amount", amount)]
        )])
    };
    let denied = sim
        .command_resolved(buyer, "points", give(buyer, 100), resolved(&[buyer], &[]))
        .await;
    assert_eq!(denied.content(), "Puan vermek için Sunucuyu Yönet iznin olmalı.");
    sim.permissions = MANAGE_GUILD;
    let given = sim
        .command_resolved(admin, "points", give(buyer, 100), resolved(&[buyer], &[]))
        .await;
    assert!(
        given.description().contains("kullanıcısına **100** 🪙 iyilik verildi")
            && given.description().contains("Yeni bakiyesi: **100**"),
        "{}",
        given.description()
    );
    let taken = sim
        .command_resolved(
            admin,
            "points",
            json!([sub("take", vec![user_option("user", buyer), int_option("amount", 5)])]),
            resolved(&[buyer], &[]),
        )
        .await;
    assert!(
        taken.description().contains("**5** 🪙 iyilik alındı")
            && taken.description().contains("Kalan bakiyesi: **95**"),
        "{}",
        taken.description()
    );
    let nothing = sim
        .command_resolved(
            admin,
            "points",
            json!([sub("take", vec![user_option("user", user), int_option("amount", 5)])]),
            resolved(&[user], &[]),
        )
        .await;
    assert!(
        nothing.content().contains("alınacak iyilik yok"),
        "{}",
        nothing.content()
    );
    sim.permissions = 0;
    let shown = sim.command(buyer, "points", json!([sub("show", vec![])])).await;
    assert_eq!(shown.title(), "🪙 İyilik");
    assert!(
        shown.description().contains(": **95** 🪙 iyilik."),
        "{}",
        shown.description()
    );
    assert!(footer(&shown).contains(". sırada"), "{}", footer(&shown));

    // The store: setup with /config store, the public listing, a refused purchase and a real one.
    let (vip_id, pricey_id) = (unique(), unique());
    let vip = sim.guild_role(vip_id, "VIP", 2);
    let pricey = sim.guild_role(pricey_id, "Whale", 3);
    sim.permissions = MANAGE_GUILD;
    let sell = |role: &Value, price: i64, stock: Option<i64>| {
        let mut options = vec![role_option("role", role), int_option("price", price)];
        options.extend(stock.map(|s| int_option("stock", s)));
        json!([group("store", sub("add", options))])
    };
    let added = sim
        .command_resolved(admin, "config", sell(&vip, 60, Some(1)), resolved(&[], &[&vip]))
        .await;
    assert!(
        added.content().starts_with("Satışta: ")
            && added.content().contains(&format!("<@&{vip_id}> · **60** 🪙 · 1 kaldı")),
        "{}",
        added.content()
    );
    sim.command_resolved(admin, "config", sell(&pricey, 500, None), resolved(&[], &[&pricey]))
        .await;
    let list = sim
        .command(admin, "config", json!([group("store", sub("list", vec![]))]))
        .await;
    assert_eq!(list.title(), "İyilik mağazası");
    assert_eq!(footer(&list), "2/25 rol");
    sim.permissions = 0;
    let store = sim.command(buyer, "store", json!([])).await;
    assert_eq!(store.title(), "Sunucu mağazası");
    assert!(
        store
            .description()
            .contains(&format!("<@&{vip_id}> · **60** 🪙 · 1 kaldı"))
            && store.description().contains(&format!("<@&{pricey_id}> · **500** 🪙")),
        "{}",
        store.description()
    );
    assert!(footer(&store).starts_with("iyilik ile öde"), "{}", footer(&store));
    let vip_item = store
        .options("st")
        .iter()
        .find(|o| o.label == "VIP")
        .expect("VIP is on sale")
        .clone();
    assert_eq!(vip_item.description.as_deref(), Some("60 iyilik · 1 kaldı"));
    let pricey_item = store
        .options("st")
        .iter()
        .find(|o| o.label == "Whale")
        .expect("Whale is on sale")
        .value
        .clone();
    sim.allow_role_changes(false);
    let failed = sim.pick(buyer, &store, "st", &vip_item.value).await;
    assert!(
        failed.content().contains("rolünü sana veremedim") && failed.content().contains("Ücret alınmadı"),
        "{}",
        failed.content()
    );
    let poor = sim.pick(buyer, &store, "st", &pricey_item).await;
    assert!(
        poor.content()
            .contains("için **500** 🪙 iyilik gerekiyor; sende **95** var."),
        "{}",
        poor.content()
    );
    sim.allow_role_changes(true);
    let bought = sim.pick(buyer, &store, "st", &vip_item.value).await;
    let receipt = bought.followups.first().expect("a private receipt");
    assert!(
        receipt.ephemeral
            && receipt
                .description()
                .contains("rolünü **60** 🪙 iyilik karşılığında aldın")
            && receipt.description().contains("Geriye **35** kaldı"),
        "{}",
        receipt.description()
    );
    // The shared store message is nobody's in particular: it follows the server's language, English here.
    assert!(bought.description().contains("sold out"), "{}", bought.description());
    // Restocked, a member who already holds the role is turned away without paying.
    sim.permissions = MANAGE_GUILD;
    sim.command_resolved(admin, "config", sell(&vip, 20, Some(5)), resolved(&[], &[&vip]))
        .await;
    sim.permissions = 0;
    sim.roles = vec![vip_id];
    let store = sim.command(buyer, "store", json!([])).await;
    let again = sim.pick(buyer, &store, "st", &vip_item.value).await;
    assert!(again.content().contains("zaten sende"), "{}", again.content());
    sim.permissions = MANAGE_GUILD;
    let removed = sim
        .command(
            admin,
            "config",
            json!([group("store", sub("remove", vec![role_option("role", &pricey)]))]),
        )
        .await;
    assert!(
        removed.content().contains("artık satışta değil"),
        "{}",
        removed.content()
    );

    // The same board and balance in English: an English capital keeps `i` as `I`.
    sim.locale = "en-US";
    sim.permissions = 0;
    let board = sim.command(user, "quests", json!([])).await;
    assert_eq!(board.title(), "Today's quests");
    assert!(board.has_button("Claim rewards"));
    let shown = sim.command(buyer, "points", json!([sub("show", vec![])])).await;
    assert_eq!(shown.title(), "🪙 Iyilik");
}
