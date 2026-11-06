//! The social and server tools in Turkish: partners, friends, hugs, reputation, guestbooks and the room
//! contest for a player whose Discord is set to Turkish, and the server tools (`/config`, role panels, custom
//! commands, giveaways, suggestions, polls, the welcome, goodbye, level-up and server log messages) for a server
//! whose default language is Turkish. The pictures they draw are saved next to the other simulator output.

use pinchora_core::i18n::Lang;

use super::*;

fn keep(sim: &Sim, reply: &Reply, name: &str) {
    let (_, png) = reply.files.first().expect("the reply carried a picture");
    std::fs::write(sim.dir.join(format!("{name}.png")), png).expect("save the picture");
}

fn footer_of(reply: &Reply) -> String {
    reply.embeds[0]
        .footer
        .as_ref()
        .map(|f| f.text.clone())
        .unwrap_or_default()
}

#[tokio::test]
async fn partners_friends_hugs_and_reputation_speak_turkish() {
    let Some(mut sim) = Sim::new("turkish_social").await else {
        return;
    };
    sim.locale = "tr";
    let (a, b, c, d, e) = (unique(), unique(), unique(), unique(), unique());

    // A partner request, its answer, the pair's page and the way out.
    let asked = sim
        .command_resolved(
            a,
            "partner",
            partner_options("propose", vec![user_option("user", b)]),
            named(&[(b, "Bea")]),
        )
        .await;
    assert_eq!(asked.title(), "💞 Partner isteği");
    assert!(
        asked.description().contains("partner olmayı teklif ediyor"),
        "{}",
        asked.description()
    );
    asked.button("Reddet");
    let stolen = sim.click(c, &asked, "Kabul et").await;
    assert!(stolen.content().contains("sana gelmedi"), "{}", stolen.content());
    let paired = sim.click(b, &asked, "Kabul et").await;
    assert_eq!(paired.title(), "💞 Yeni partnerler");
    keep(&sim, &paired, "partners-new");
    sim.app.db.record_interaction("hug", a, b).await.unwrap();
    let view = sim.command(a, "partner", partner_options("view", vec![])).await;
    assert!(
        view.field("Birbirine gönderdikleri").contains("🤗 1 sarılma"),
        "{}",
        view.field("Birbirine gönderdikleri")
    );
    assert!(!view.field("Şu tarihten beri birlikte").is_empty());
    assert!(view.field("Evleri").contains("beğeni"), "{}", view.field("Evleri"));
    keep(&sim, &view, "partners-view");
    let alone = sim.command(c, "partner", partner_options("view", vec![])).await;
    assert!(alone.content().contains("partnerin yok"), "{}", alone.content());
    let confirm = sim.command(a, "partner", partner_options("leave", vec![])).await;
    assert_eq!(confirm.title(), "Partnerlikten ayrılıyor musun?");
    confirm.button("Birlikte kal");
    let left = sim.click(a, &confirm, "Ayrıl").await;
    assert!(
        left.content().contains("artık partner değilsiniz"),
        "{}",
        left.content()
    );

    // Friends: the request, the list.
    let asked = sim
        .command_resolved(
            c,
            "friends",
            json!([sub("add", vec![user_option("user", d)])]),
            named(&[(d, "Dora")]),
        )
        .await;
    assert_eq!(asked.title(), "🤝 Arkadaşlık isteği");
    let made = sim.click(d, &asked, "Kabul et").await;
    assert_eq!(made.title(), "🤝 Yeni arkadaşlar");
    let list = sim.command(c, "friends", json!([sub("list", vec![])])).await;
    assert_eq!(list.title(), "👥 Arkadaşların");
    assert!(list.description().contains(&mention(d)), "{}", list.description());
    assert!(footer_of(&list).contains("1/"), "{}", footer_of(&list));
    let nobody = sim.command(e, "friends", json!([sub("list", vec![])])).await;
    assert!(
        nobody.description().contains("Henüz arkadaşın yok"),
        "{}",
        nobody.description()
    );

    // Hugs and the rest: the line above the picture, the count, the way back.
    for (command, said, noun, back) in [
        ("hug", "sarılıyor", "sarılma", "Sen de sarıl"),
        ("pat", "başını okşuyor", "okşama", "Sen de okşa"),
        ("wave", "el sallıyor", "el sallama", "Sen de el salla"),
        ("highfive", "beşlik çakıyor", "beşlik", "Sen de beşlik çak"),
    ] {
        let sender = unique();
        let reply = sim
            .command_resolved(sender, command, json!([user_option("user", b)]), named(&[(b, "Bea")]))
            .await;
        assert!(reply.content().contains(said), "{command}: {}", reply.content());
        assert!(
            reply.description().contains(&format!("giden **1.** {noun}")),
            "{}",
            reply.description()
        );
        let footer = footer_of(&reply);
        assert!(
            footer.contains("gönderdi") && footer.contains("aldı") && footer.contains("/character ile"),
            "{footer}"
        );
        reply.button(back);
    }
    let sender = unique();
    let selfie = sim
        .command_resolved(
            sender,
            "hug",
            json!([user_option("user", sender)]),
            named(&[(sender, "Sim")]),
        )
        .await;
    assert!(
        selfie.content().contains("Kendine sarılamazsın"),
        "{}",
        selfie.content()
    );
    let first = sim
        .command_resolved(sender, "wave", json!([user_option("user", b)]), named(&[(b, "Bea")]))
        .await;
    assert!(first.content().contains("el sallıyor"), "{}", first.content());
    // Not a race against the clock: the wait is put on by hand, so a slow machine cannot end it early.
    let key = format!("social:{sender}");
    sim.app.cache.release_cooldown(&key).await.unwrap();
    sim.app
        .cache
        .try_cooldown(&key, std::time::Duration::from_secs(60))
        .await
        .unwrap();
    let hurried = sim
        .command_resolved(sender, "wave", json!([user_option("user", b)]), named(&[(b, "Bea")]))
        .await;
    assert!(hurried.content().contains("Yavaş ol!"), "{}", hurried.content());

    // Reputation, the about-me line, and the round trip.
    let given = give_rep(&mut sim, sender, b, &[]).await;
    assert!(
        given.description().contains("bir itibar puanı verdi"),
        "{}",
        given.description()
    );
    let again = give_rep(&mut sim, sender, b, &[]).await;
    assert!(again.content().contains("İtibarı yeniden"), "{}", again.content());
    let yourself = give_rep(&mut sim, sender, sender, &[]).await;
    assert!(
        yourself.content().contains("başka birinden gelmeli"),
        "{}",
        yourself.content()
    );
    let bio = sim
        .command(sender, "bio", json!([text_option("text", "Merhaba!")]))
        .await;
    assert!(bio.content().contains("güncellendi"), "{}", bio.content());
    let cleared = sim.command(sender, "bio", json!([text_option("text", "-")])).await;
    assert!(cleared.content().contains("silindi"), "{}", cleared.content());
    let pong = sim.command(sender, "ping", json!([])).await;
    assert_eq!(pong.title(), "Pong!");
    assert!(pong.field("Etkileşim").ends_with("ms"), "{}", pong.field("Etkileşim"));
    assert!(pong.field("Veritabanı").ends_with("ms"));
}

#[tokio::test]
async fn guestbooks_and_the_room_contest_speak_turkish() {
    let Some(mut sim) = Sim::new("turkish_guestbook_contest").await else {
        return;
    };
    sim.locale = "tr";
    let (host, guest, stranger) = (unique(), unique(), unique());
    sim.command(host, "house", json!([{ "name": "view", "type": 1 }])).await;

    let visit = sim
        .command_resolved(guest, "house", visit_options(host), resolved(&[host], &[]))
        .await;
    let form = sim.click(guest, &visit, "Deftere yaz").await;
    assert_eq!(
        form.modal.as_ref().map(|m| m.0.as_str()),
        Some(format!("gb:n:{host}").as_str())
    );
    let signed = sim
        .submit_fields(guest, &form, &[("note", "Ne güzel bir oda! Akvaryum favorim.")])
        .await;
    assert!(
        signed.ephemeral && signed.content().contains("Yazıldı!"),
        "{}",
        signed.content()
    );
    let empty = sim.submit_fields(guest, &form, &[("note", "@everyone")]).await;
    assert!(empty.content().contains("boş kaldı"), "{}", empty.content());
    let again = sim.click(guest, &visit, "Deftere yaz").await;
    assert!(again.content().contains("bugün zaten yazdın"), "{}", again.content());
    let nope = sim.click(stranger, &visit, "Deftere yaz").await;
    assert!(nope.content().contains("Önce `/house visit`"), "{}", nope.content());
    let own = sim.click(host, &visit, "Deftere yaz").await;
    assert!(own.content().contains("kendi ziyaretçi defterin"), "{}", own.content());

    let book = sim
        .command(host, "house", json!([{ "name": "guestbook", "type": 1 }]))
        .await;
    assert_eq!(book.title(), "📖 Ziyaretçi defterin");
    assert!(book.description().contains("**yeni**"), "{}", book.description());
    assert!(
        book.description().contains("Akvaryum favorim"),
        "{}",
        book.description()
    );
    assert!(!book.field("Son ziyaretçiler").is_empty());
    assert_eq!(footer_of(&book), "1 not");
    assert!(book.options("gb:d").len() == 1, "the delete menu");
    let public = sim
        .command_resolved(
            stranger,
            "house",
            json!([sub("guestbook", vec![user_option("user", host)])]),
            named(&[(host, "Hugo")]),
        )
        .await;
    assert!(public.title().contains("ziyaretçi defteri"), "{}", public.title());

    // The room contest: entering, the week's page, a ballot, and last week's winners (none yet).
    let (a, b) = (unique(), unique());
    // The helper clicks the editor's English buttons.
    sim.locale = "en-US";
    furnish(&mut sim, a, &["Rugs", "Seating", "Tables", "Plants"]).await;
    furnish(&mut sim, b, &["Beds", "Lighting", "Rugs"]).await;
    sim.locale = "tr";
    let empty = sim.command(stranger, "house", contest("enter", vec![])).await;
    assert!(empty.content().contains("boş. Önce"), "{}", empty.content());
    let entered = sim.command(a, "house", contest("enter", vec![])).await;
    assert!(
        entered.title().contains("oda yarışmasına katıldı"),
        "{}",
        entered.title()
    );
    assert!(entered.description().contains("parça"), "{}", entered.description());
    keep(&sim, &entered, "contest-entered");
    let updated = sim.command(a, "house", contest("enter", vec![])).await;
    assert!(updated.title().contains("katılımını güncelledi"), "{}", updated.title());
    sim.command(b, "house", contest("enter", vec![])).await;
    let view = sim.command(a, "house", contest("view", vec![])).await;
    assert!(view.title().starts_with("🏆 Oda yarışması: "), "{}", view.title());
    assert_eq!(view.field("Katılımlar"), "2");
    assert!(
        view.field("Senin katılımın").contains(". oda ·"),
        "{}",
        view.field("Senin katılımın")
    );
    assert!(
        view.field("Geçen hafta").contains("henüz biten"),
        "{}",
        view.field("Geçen hafta")
    );
    assert!(view.description().contains("Haftanın Evi"), "{}", view.description());
    let ballot = sim.click(a, &view, "Oy ver").await;
    ballot.button("💖 Oy ver");
    ballot.button("Atla");
    assert!(
        footer_of(&ballot).contains("Oda başına bir oy"),
        "{}",
        footer_of(&ballot)
    );
    keep(&sim, &ballot, "contest-ballot");
    let voted = sim.click(a, &ballot, "💖 Oy ver").await;
    assert!(
        voted.description().contains("Oy sayıldı") || voted.content().contains("puanladın"),
        "{} / {}",
        voted.description(),
        voted.content()
    );
    let winners = sim.command(a, "house", contest("winners", vec![])).await;
    assert!(winners.content().contains("henüz biten"), "{}", winners.content());
}

#[tokio::test]
async fn the_server_tools_answer_in_turkish() {
    let Some(mut sim) = Sim::new("turkish_server_tools").await else {
        return;
    };
    let admin = unique();
    sim.own_server(admin);
    sim.locale = "tr";
    sim.permissions = MANAGE_GUILD;
    let config = |sub_command: Value| json!([sub_command]);
    let (log, bye, ideas, lounge) = (unique(), unique(), unique(), unique());

    // /config show: every field in Turkish, the defaults too.
    let shown = sim.command(admin, "config", config(sub("show", vec![]))).await;
    assert_eq!(shown.title(), "Sunucu ayarları");
    assert!(shown.field("Karşılama kartları").contains("**kapalı** · ayarlanmadı"));
    assert!(
        shown.field("Karşılama kartları").contains("sunucusuna hoş geldin"),
        "{}",
        shown.field("Karşılama kartları")
    );
    assert!(shown.field("Seviye sistemi").contains("seviyeye** ulaştı"));
    assert!(
        shown
            .field("Seviye sistemi")
            .contains("seviye atlamaları üyenin yazdığı kanalda")
    );
    assert!(shown.field("XP kuralları ve ses XP'si").contains("dakikada"));
    assert!(shown.field("Seviye rolleri").contains("biriktir"));
    assert!(shown.field("Sunucu puanları").contains("mesaj başına"));
    assert!(shown.field("Katılma rolü").contains("kapalı"));
    assert!(shown.field("Veda").contains("kapalı"));
    assert!(shown.field("Sunucu kaydı").contains("kapalı"));
    assert!(shown.field("Öneriler").contains("kapalı"));

    // The welcome card preview, with the Turkish chrome drawn on it.
    let card = sim
        .command(
            admin,
            "config",
            config(sub(
                "welcome-card",
                vec![text_option("theme", "sunset"), text_option("title", "Hoş geldin")],
            )),
        )
        .await;
    assert_eq!(card.title(), "Karşılama kartı önizlemesi");
    keep(&sim, &card, "welcome-card-preview");
    let bad = sim
        .command(
            admin,
            "config",
            config(sub("welcome-card", vec![text_option("color", "zzz")])),
        )
        .await;
    assert!(bad.content().contains("gibi yazılır"), "{}", bad.content());

    // Join role, goodbye, log, suggestions: the summaries and the refusals.
    let newbie = sim.guild_role(unique(), "Yeni", 2);
    let above = role_json(unique(), "Patron", BOT_ROLE_POSITION + 5, false);
    let refused = sim
        .command_resolved(
            admin,
            "config",
            config(sub("join-role", vec![role_option("role", &above)])),
            resolved(&[], &[&above]),
        )
        .await;
    assert!(
        refused.content().contains("en yüksek rolümün üstünde"),
        "{}",
        refused.content()
    );
    let set = sim
        .command_resolved(
            admin,
            "config",
            config(sub("join-role", vec![role_option("role", &newbie)])),
            resolved(&[], &[&newbie]),
        )
        .await;
    assert!(set.content().contains("Botlar atlanır"), "{}", set.content());
    let goodbye = sim
        .command_resolved(
            admin,
            "config",
            config(sub("goodbye", vec![channel_option("channel", bye)])),
            resolved_channels(&[bye]),
        )
        .await;
    assert!(
        goodbye.content().contains(&format!("**açık** · <#{bye}>")) && goodbye.content().contains("ayrıldı"),
        "{}",
        goodbye.content()
    );
    let clash = sim
        .command_resolved(
            admin,
            "config",
            config(sub(
                "goodbye",
                vec![channel_option("channel", bye), bool_option("off", true)],
            )),
            resolved_channels(&[bye]),
        )
        .await;
    assert!(clash.content().contains("tek başına"), "{}", clash.content());
    let logging = sim
        .command_resolved(
            admin,
            "config",
            config(sub("log", vec![channel_option("channel", log)])),
            resolved_channels(&[log]),
        )
        .await;
    assert!(logging.content().contains("ad değişiklikleri"), "{}", logging.content());
    let suggestions = sim
        .command_resolved(
            admin,
            "config",
            config(sub("suggestions", vec![channel_option("channel", ideas)])),
            resolved_channels(&[ideas]),
        )
        .await;
    assert!(
        suggestions.content().contains(&format!("**açık** · <#{ideas}>")),
        "{}",
        suggestions.content()
    );

    // Level roles and reputation roles.
    let novice = sim.guild_role(unique(), "Acemi", 3);
    let added = sim
        .command_resolved(
            admin,
            "config",
            json!([group(
                "level-roles",
                sub("add", vec![int_option("level", 5), role_option("role", &novice)])
            )]),
            resolved(&[], &[&novice]),
        )
        .await;
    assert!(
        added.content().contains("**5.** seviyeye ulaşan"),
        "{}",
        added.content()
    );
    let listed = sim
        .command(admin, "config", json!([group("level-roles", sub("list", vec![]))]))
        .await;
    assert_eq!(listed.title(), "Seviye rolleri");
    assert!(footer_of(&listed).contains("biriktir"), "{}", footer_of(&listed));
    let mode = sim
        .command(
            admin,
            "config",
            json!([group("level-roles", sub("mode", vec![text_option("mode", "replace")]))]),
        )
        .await;
    assert!(mode.content().contains("yerine geçiyor"), "{}", mode.content());
    let rep = sim
        .command_resolved(
            admin,
            "config",
            json!([group(
                "rep-roles",
                sub("add", vec![int_option("reputation", 10), role_option("role", &novice)])
            )]),
            resolved(&[], &[&novice]),
        )
        .await;
    assert!(rep.content().contains("**10** itibara ulaşan"), "{}", rep.content());
    let rep_list = sim
        .command(admin, "config", json!([group("rep-roles", sub("list", vec![]))]))
        .await;
    assert_eq!(rep_list.title(), "İtibar rolleri");

    // XP rules.
    let multiplied = sim
        .command_resolved(
            admin,
            "config",
            json!([group(
                "xp",
                sub(
                    "multiplier",
                    vec![number_option("value", 4.5), channel_option("channel", lounge)]
                )
            )]),
            resolved_channels(&[lounge]),
        )
        .await;
    assert!(
        multiplied.content().contains("**×4.5** XP ve puan"),
        "{}",
        multiplied.content()
    );
    let xp = sim
        .command(admin, "config", json!([group("xp", sub("show", vec![]))]))
        .await;
    assert_eq!(xp.title(), "XP kuralları");
    assert!(xp.field("Ses XP'si").contains("dakikada"), "{}", xp.field("Ses XP'si"));
    let voice = sim
        .command(
            admin,
            "config",
            json!([group("xp", sub("voice", vec![bool_option("enabled", true)]))]),
        )
        .await;
    assert_eq!(voice.title(), "Ses XP'si");
    assert!(voice.description().contains("**açık**"), "{}", voice.description());

    // Modules: groups, names and a switch by its Turkish name.
    let modules = sim
        .command(admin, "config", json!([group("modules", sub("show", vec![]))]))
        .await;
    assert_eq!(modules.title(), "Bölümler");
    assert!(
        modules.field("Sunucu araçları").contains("Anketler"),
        "{}",
        modules.field("Sunucu araçları")
    );
    assert!(modules.field("Oyuncular").contains("**Profiller ve sıralar** · açık"));
    let off = sim
        .command(
            admin,
            "config",
            json!([group(
                "modules",
                sub(
                    "toggle",
                    vec![text_option("module", "Anketler"), bool_option("enabled", false)]
                )
            )]),
        )
        .await;
    assert_eq!(off.content(), "⚫ **Anketler** · kapalı");
    let on = sim
        .command(
            admin,
            "config",
            json!([group(
                "modules",
                sub(
                    "toggle",
                    vec![text_option("module", "polls"), bool_option("enabled", true)]
                )
            )]),
        )
        .await;
    assert_eq!(on.content(), "🟢 **Anketler** · açık");
    let nonsense = sim
        .command(
            admin,
            "config",
            json!([group(
                "modules",
                sub(
                    "toggle",
                    vec![text_option("module", "yok böyle"), bool_option("enabled", true)]
                )
            )]),
        )
        .await;
    assert!(
        nonsense.content().contains("Bu bir bölüm değil"),
        "{}",
        nonsense.content()
    );

    // Role panels.
    let red = sim.guild_role(unique(), "Kırmızı", 2);
    let draft = sim
        .command(
            admin,
            "roles",
            json!([sub("panel", vec![text_option("title", "Renkler")])]),
        )
        .await;
    assert!(draft.content().contains("bir taslak"), "{}", draft.content());
    let placed = sim
        .command_resolved(
            admin,
            "roles",
            json!([sub(
                "add",
                vec![
                    text_option("panel", "Renkler"),
                    role_option("role", &red),
                    text_option("label", "Kırmızı")
                ]
            )]),
            resolved(&[], &[&red]),
        )
        .await;
    assert!(placed.content().contains("(1/25)"), "{}", placed.content());
    let missing = sim
        .command_resolved(
            admin,
            "roles",
            json!([sub(
                "post",
                vec![text_option("panel", "Yok"), channel_option("channel", lounge)]
            )]),
            resolved_channels(&[lounge]),
        )
        .await;
    assert!(
        missing.content().contains("adında bir panel yok"),
        "{}",
        missing.content()
    );
    // The panel with its button, posted: the message speaks the server's language (English here), the
    // refusals and answers to the manager speak theirs.
    let posted = sim
        .command_resolved(
            admin,
            "roles",
            json!([sub(
                "post",
                vec![text_option("panel", "Renkler"), channel_option("channel", lounge)]
            )]),
            resolved_channels(&[lounge]),
        )
        .await;
    assert!(posted.content().contains("paneli"), "{}", posted.content());
    let posts = sim.posts();
    let panel_post = only(&posts, lounge, Method::POST);
    assert_eq!(
        panel_post.embed()["footer"]["text"],
        "Click a button to get a role; click it again to drop it"
    );

    // Custom commands: the form, the list, the card of one, and running it.
    let form = sim
        .command(
            admin,
            "customcommand",
            json!([sub("create", vec![text_option("name", "kurallar")])]),
        )
        .await;
    assert!(form.modal.is_some(), "the create form opens");
    let created = sim
        .submit_fields(
            admin,
            &form,
            &[
                ("response", "Kibar ol, {user}!"),
                ("description", "Sunucu kuralları"),
                ("cooldown", ""),
            ],
        )
        .await;
    assert!(created.content().contains("oluşturuldu"), "{}", created.content());
    let again = sim
        .command(
            admin,
            "customcommand",
            json!([sub("create", vec![text_option("name", "kurallar")])]),
        )
        .await;
    assert!(again.content().contains("zaten var"), "{}", again.content());
    let bad_name = sim
        .command(
            admin,
            "customcommand",
            json!([sub("create", vec![text_option("name", "Çok Kötü!")])]),
        )
        .await;
    assert!(bad_name.content().contains("küçük harfler"), "{}", bad_name.content());
    let list = sim.command(admin, "customcommand", json!([sub("list", vec![])])).await;
    assert!(list.title().starts_with("Özel komutlar (1/"), "{}", list.title());
    assert!(list.description().contains("0 kez kullanıldı") || list.description().contains("kez"));
    let info = sim
        .command(
            admin,
            "customcommand",
            json!([sub("info", vec![text_option("name", "kurallar")])]),
        )
        .await;
    assert_eq!(info.field("Durum"), "açık");
    assert_eq!(info.field("Yanıt şekli"), "kanalda");
    assert_eq!(info.field("Yanıtlar"), "1 yanıt");
    let ran = sim
        .command(admin, "tag", json!([text_option("name", "kurallar")]))
        .await;
    assert!(ran.content().contains("Kibar ol"), "{}", ran.content());
    let unknown = sim.command(admin, "tag", json!([text_option("name", "yok")])).await;
    assert!(unknown.content().contains("özel komut yok"), "{}", unknown.content());
}

#[tokio::test]
async fn a_turkish_server_hears_its_events_in_turkish() {
    let Some(mut sim) = Sim::new("turkish_server_events").await else {
        return;
    };
    let _turn = EVENTS.lock().await;
    let admin = unique();
    sim.own_server(admin);
    sim.channel = unique();
    let channel = sim.channel;
    sim.app.set_guild_lang(sim.guild, Some(Lang::Tr)).await.unwrap();
    sim.locale = "tr";
    sim.permissions = MANAGE_GUILD;
    let (welcome, bye, log, ideas, chatty) = (unique(), unique(), unique(), unique(), unique());
    let config = |sub_command: Value| json!([sub_command]);

    // Welcome card, goodbye and the log, all in the server's language.
    sim.command_resolved(
        admin,
        "config",
        config(sub(
            "welcome",
            vec![bool_option("enabled", true), channel_option("channel", welcome)],
        )),
        resolved_channels(&[welcome]),
    )
    .await;
    sim.command_resolved(
        admin,
        "config",
        config(sub("goodbye", vec![channel_option("channel", bye)])),
        resolved_channels(&[bye]),
    )
    .await;
    sim.command_resolved(
        admin,
        "config",
        config(sub("log", vec![channel_option("channel", log)])),
        resolved_channels(&[log]),
    )
    .await;
    let (alice, robot) = (unique(), unique());
    sim.join(alice, "Alice", false, None).await;
    let posts = sim.posts();
    let hello = in_channel(&posts, welcome);
    let [hello] = &hello[..] else {
        panic!("expected one welcome, got {}", hello.len());
    };
    assert_eq!(
        hello.content(),
        format!("**{GUILD_NAME}** sunucusuna hoş geldin, <@{alice}>!")
    );
    assert_eq!(hello.files.len(), 1, "the card");
    let joined = in_channel(&posts, log);
    assert_eq!(joined.iter().map(|p| p.title()).collect::<Vec<_>>(), ["Üye katıldı"]);
    assert!(joined[0].field("Hesap oluşturuldu").starts_with("<t:"));
    sim.join(robot, "RoboBot", true, None).await;
    assert_eq!(sim.posts().iter().map(Post::title).collect::<Vec<_>>(), ["Bot katıldı"]);
    sim.leave(alice, "Alice", false).await;
    let posts = sim.posts();
    assert_eq!(
        in_channel(&posts, bye)[0].content(),
        format!("👋 **Alice**, **{GUILD_NAME}** sunucusundan ayrıldı.")
    );
    assert_eq!(in_channel(&posts, log)[0].title(), "Üye ayrıldı");
    sim.update_member(alice, "alice", Some("Zed")).await;
    sim.update_member(alice, "alice", Some("Zedd")).await;
    // (Names are only noted where the log reports them; the entries above come from the join and leave.)
    sim.posts();

    // A level-up banner and its line.
    let up = unique();
    sim.app.db.add_message_xp(sim.guild, up, 99, 0).await.unwrap();
    sim.say_in(up, chatty, &[]).await;
    let posts = sim.posts();
    let said = in_channel(&posts, chatty);
    assert!(
        said.iter()
            .any(|p| p.content() == format!("🎉 <@{up}> **1. seviyeye** ulaştı!")),
        "{:?}",
        said.iter().map(|p| p.content()).collect::<Vec<_>>()
    );

    // A giveaway: the message, the entry, the end.
    fund(&sim, admin, 1_000).await;
    let started = giveaway_cmd(
        &mut sim,
        admin,
        "start",
        giveaway_start("2h", 2, "Altın kabuk", Some(50)),
    )
    .await;
    assert!(started.content().contains("kanalında başladı"), "{}", started.content());
    assert!(started.content().contains("100 kabuk"), "{}", started.content());
    let posts = sim.posts();
    let post = only(&posts, channel, Method::POST);
    assert_eq!(post.title(), "Altın kabuk");
    let text = post.embed()["description"].as_str().unwrap();
    assert!(
        text.contains("Katılmak için **Katıl**'a bas")
            && text.contains("Düzenleyen:")
            && text.contains("Ödül: kişi başı **50** kabuk"),
        "{text}"
    );
    assert_eq!(post.field("Katılım"), "0");
    let message = reply_of(post);
    let entered = sim.click(unique(), &message, "Katıl").await;
    assert!(entered.content().contains("Katıldın!"), "{}", entered.content());
    assert!(entered.content().contains("1 katılım"), "{}", entered.content());
    let own = sim.click(admin, &message, "Katıl").await;
    assert!(own.content().contains("sen düzenliyorsun"), "{}", own.content());
    let listed = giveaway_cmd(&mut sim, admin, "list", vec![]).await;
    assert_eq!(listed.title(), "Süren çekilişler");
    let id = sim.app.db.running_giveaways(sim.guild).await.unwrap()[0].id;
    let ended = giveaway_cmd(&mut sim, admin, "end", vec![int_option("id", id)]).await;
    assert!(
        ended.content().contains("numaralı çekiliş bitti"),
        "{}",
        ended.content()
    );
    let posts = sim.posts();
    let announced = in_channel(&posts, channel);
    assert!(
        announced
            .iter()
            .any(|p| p.content().starts_with("🎉 Tebrikler") && p.content().contains("Altın kabuk")),
        "{:?}",
        announced.iter().map(|p| p.content()).collect::<Vec<_>>()
    );
    let missing = giveaway_cmd(&mut sim, admin, "end", vec![int_option("id", 987_654)]).await;
    assert!(
        missing.content().contains("numaralı bir çekiliş yok"),
        "{}",
        missing.content()
    );

    // A suggestion in the suggestions channel, then the manager's verdict.
    sim.command_resolved(
        admin,
        "config",
        config(sub("suggestions", vec![channel_option("channel", ideas)])),
        resolved_channels(&[ideas]),
    )
    .await;
    let author = unique();
    DM_OVERRIDES.lock().unwrap().insert(author, false);
    let sent = sim
        .command(author, "suggest", json!([text_option("text", "Bir bahçe ekleyelim")]))
        .await;
    assert!(sent.content().contains("yayında"), "{}", sent.content());
    let posts = sim.posts();
    let idea = only(&posts, ideas, Method::POST);
    assert_eq!(idea.title(), "Öneri #1");
    assert_eq!(idea.field("Öneren"), format!("<@{author}>"));
    assert!(
        idea.embed()["footer"]["text"]
            .as_str()
            .unwrap()
            .contains("Düğmelerle oy ver"),
        "{}",
        idea.embed()
    );
    let verdict = sim
        .command(
            admin,
            "suggestion",
            json!([sub(
                "approve",
                vec![int_option("id", 1), text_option("reason", "Harika fikir")]
            )]),
        )
        .await;
    assert!(verdict.content().contains("onaylandı"), "{}", verdict.content());
    let posts = sim.posts();
    let edited = only(&posts, ideas, Method::PATCH);
    assert_eq!(edited.field("Onaylayan"), format!("<@{admin}>"));
    assert_eq!(edited.field("Gerekçe"), "Harika fikir");
    let twice = sim
        .command(admin, "suggestion", json!([sub("approve", vec![int_option("id", 1)])]))
        .await;
    assert!(twice.content().contains("zaten onaylandı"), "{}", twice.content());

    // A poll: the message in the server's language, the chart in the clicker's.
    let opened = poll_cmd(
        &mut sim,
        admin,
        "Pizza mı, tacos mu?",
        "Pizza | Tacos | Suşi",
        Some("1h"),
    )
    .await;
    assert!(opened.content().contains("anket"), "{}", opened.content());
    let posts = sim.posts();
    let poll = only(&posts, channel, Method::POST);
    let text = poll.embed()["description"].as_str().unwrap();
    assert!(text.contains("Soran:") && text.contains("Bitiş:"), "{text}");
    let message = reply_of(poll);
    let vote = sim.click(unique(), &message, "Tacos").await;
    assert!(vote.content().contains("oyun kaydedildi"), "{}", vote.content());
    let chart = sim.click(unique(), &message, "Sonuçlar").await;
    assert_eq!(chart.files[0].0, "poll.png");
    assert!(
        chart.description().contains("saniyede bir yenilenir"),
        "{}",
        chart.description()
    );
    keep(&sim, &chart, "poll-results");
    // The chart answers the member who pressed the button, whatever the server speaks.
    sim.locale = "en-US";
    let english = sim.click(unique(), &message, "Sonuçlar").await;
    assert!(
        english.description().contains("Results refresh every"),
        "{}",
        english.description()
    );
    sim.locale = "tr";
    let bad = poll_cmd(&mut sim, admin, "Soru?", "Yalnızca bir", None).await;
    assert!(bad.content().contains("iki ila beş cevap"), "{}", bad.content());
}
