use super::*;

// /Adopting

pub(super) async fn adopt_menu(cx: &Cx, user: u64) -> anyhow::Result<()> {
    let lang = cx.lang();
    if !cx.app.db.pets(user).await?.is_empty() {
        return cx.reply_ephemeral(t!(lang, "pet.adopt.have-one")).await;
    }
    let content = Content::get();
    let options: Vec<SelectMenuOption> = content
        .pets
        .iter()
        .filter(|p| p.starter)
        .map(|p| SelectMenuOption {
            default: false,
            description: Some(
                content
                    .description(&p.id, lang)
                    .unwrap_or_default()
                    .chars()
                    .take(100)
                    .collect(),
            ),
            emoji: None,
            label: content.name(&p.id, lang).into_owned(),
            value: p.id.clone(),
        })
        .collect();
    if options.is_empty() {
        return cx.reply_ephemeral(t!(lang, "pet.adopt.none-ready")).await;
    }
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(lang, "pet.adopt.title"))
        .description(t!(lang, "pet.adopt.description"))
        .build();
    cx.reply(InteractionResponseData {
        embeds: Some(vec![embed]),
        components: Some(vec![select(
            "pa".to_owned(),
            &t!(lang, "pet.adopt.placeholder"),
            options,
        )]),
        flags: Some(MessageFlags::EPHEMERAL),
        ..Default::default()
    })
    .await
}

/// `pa` select: adopt the chosen starter (only when the player has no pets).
pub(crate) async fn on_adopt(cx: &Cx, value: Option<&str>) -> anyhow::Result<()> {
    let lang = cx.lang();
    let user = cx.author()?.id.get();
    let content = Content::get();
    let Some(species) = value.and_then(|v| content.pet(v)).filter(|p| p.starter) else {
        return cx.reply_ephemeral(t!(lang, "pet.adopt.pick-starter")).await;
    };
    if !cx.app.db.pets(user).await?.is_empty() {
        return cx.reply_ephemeral(t!(lang, "pet.adopt.already-adopted")).await;
    }
    // A new pet is named after its species, in the adopter's language; it is theirs to rename.
    let name = content.name(&species.id, lang);
    let pet = cx.app.db.adopt(user, &species.id, &name).await?;
    let data = pet_card(&cx.app, lang, &pet, Some(t!(lang, "pet.adopt.welcome", name = name))).await?;
    cx.update(data).await?;
    customize::announce(cx).await;
    Ok(())
}

pub(super) async fn capsule(cx: &Cx, user: u64) -> anyhow::Result<()> {
    let lang = cx.lang();
    let content = Content::get();
    let Some(balance) = cx.app.db.spend_shells(user, CAPSULE_PRICE, "pet_capsule").await? else {
        let odds = pets::odds(content)
            .into_iter()
            .map(|(r, p)| format!("{} {}%", r.label_in(lang), decimal_in(lang, p * 100.0, 1)))
            .collect::<Vec<_>>()
            .join(" · ");
        return cx
            .reply_ephemeral(t!(
                lang,
                "pet.capsule.odds",
                price = thousands_in(lang, CAPSULE_PRICE),
                odds = odds
            ))
            .await;
    };
    let Some(species) = pets::hatch(content, rand::random::<f64>()) else {
        cx.app.db.earn_shells(user, CAPSULE_PRICE, "pet_capsule_refund").await?;
        return cx.reply_ephemeral(t!(lang, "pet.capsule.restocking")).await;
    };
    let name = content.name(&species.id, lang);
    if cx.app.db.has_species(user, &species.id).await? {
        let balance = cx.app.db.earn_shells(user, DUPLICATE_REFUND, "pet_duplicate").await?;
        let embed = EmbedBuilder::new()
            .color(WARN)
            .title(t!(
                lang,
                "pet.capsule.duplicate-title",
                rarity = lower(lang, species.rarity.label_in(lang)),
                name = name
            ))
            .description(t!(
                lang,
                "pet.capsule.duplicate",
                refund = DUPLICATE_REFUND,
                balance = thousands_in(lang, balance)
            ))
            .build();
        return cx.reply_embed(embed).await;
    }
    let pet = cx.app.db.adopt(user, &species.id, &name).await?;
    let note = t!(
        lang,
        "pet.capsule.opened",
        rarity = species.rarity.label_in(lang),
        name = name,
        balance = thousands_in(lang, balance)
    );
    let data = pet_card(&cx.app, lang, &pet, Some(note)).await?;
    cx.reply(data).await?;
    customize::announce(cx).await;
    Ok(())
}

/// `ps` select: switch the active pet. Kept for nursery lists posted before the grid card.
pub(crate) async fn on_switch(cx: &Cx, value: Option<&str>) -> anyhow::Result<()> {
    let lang = cx.lang();
    let user = cx.author()?.id.get();
    let pet_id: i64 = value.and_then(|v| v.parse().ok()).unwrap_or(0);
    if cx.app.db.set_active_pet(user, pet_id).await? {
        cx.update(InteractionResponseData {
            content: Some(t!(lang, "pet.switch.changed")),
            embeds: Some(Vec::new()),
            components: Some(Vec::new()),
            ..Default::default()
        })
        .await
    } else {
        cx.reply_ephemeral(t!(lang, "pet.switch.not-yours")).await
    }
}
