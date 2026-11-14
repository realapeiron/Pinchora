use super::*;

// /Sending

/// What to do with a reminder that is due.
enum Delivery {
    Send {
        title: String,
        text: String,
    },
    /// Not wanted any more, or about something that no longer holds.
    Drop,
    /// Not time yet after all.
    Later(DateTime<Utc>),
}

/// The language a due reminder is written in. There is no interaction to ask: the player's
/// `/language` pick, else the language their command was answered in when the message was scheduled
/// (the automatic kinds keep it in their `body`), else English.
async fn delivery_lang(app: &App, r: &ReminderRow) -> Lang {
    let stored = if r.kind == TEXT_REMINDER {
        Lang::from_code(&r.language)
    } else {
        Lang::from_code(&r.body)
    };
    match app.user_lang_choice(r.user_id as u64).await {
        Some(lang) => lang,
        None => stored.unwrap_or_default(),
    }
}

async fn plan(app: &App, r: &ReminderRow, now: DateTime<Utc>, lang: Lang) -> anyhow::Result<Delivery> {
    let user = r.user_id as u64;
    if r.kind == TEXT_REMINDER {
        return Ok(Delivery::Send {
            title: t!(lang, "remind.text-title"),
            text: r.body.clone(),
        });
    }
    if let Some(minutes) = r.kind.strip_prefix("event_").and_then(|v| v.parse::<i32>().ok()) {
        return Ok(
            match crate::community::event_rsvp::reminder_plan(app, r, minutes, lang).await? {
                Some((title, text)) => Delivery::Send { title, text },
                None => Delivery::Drop,
            },
        );
    }
    // Everything else only goes out while the player still wants it.
    if !app.db.has_optin(user, &r.kind).await? {
        return Ok(Delivery::Drop);
    }
    Ok(match r.kind.as_str() {
        "daily" | "weekly" | "monthly" => Delivery::Send {
            title: t!(lang, &format!("remind.title.{}", r.kind)),
            text: format!(
                "{}\n{}",
                t!(lang, "remind.collect", cmd = crate::infra::routing::canonical(&r.kind)),
                t!(lang, "remind.turn-off", what = r.kind)
            ),
        },
        "trip" => match app.db.pets(user).await?.into_iter().find(|p| p.id == r.ref_id) {
            Some(pet) if pet.has_souvenir(now) => Delivery::Send {
                title: t!(lang, "remind.trip-title", name = pet.name),
                text: format!(
                    "{}\n{}",
                    t!(lang, "remind.trip-text"),
                    t!(lang, "remind.turn-off", what = "trip")
                ),
            },
            _ => Delivery::Drop,
        },
        "pet" => match app.db.pets(user).await?.into_iter().find(|p| p.id == r.ref_id) {
            Some(pet) if pet.active && !pet.is_away(now) && !pet::is_asleep(&pet, now) => {
                let due = pet_due(&pet);
                if due > now {
                    Delivery::Later(due)
                } else {
                    let stored = Needs {
                        hunger: pet.hunger,
                        fun: pet.fun,
                        energy: pet.energy,
                        hygiene: pet.hygiene,
                    };
                    let hours = (now - pet.needs_at).num_seconds() as f32 / 3600.0;
                    let wants = t!(lang, &format!("remind.wants.{}", stored.after(hours).want_key()));
                    Delivery::Send {
                        title: t!(lang, "remind.pet-title", name = pet.name),
                        text: format!(
                            "{}\n{}",
                            t!(lang, "remind.pet-text", wants = wants),
                            t!(lang, "remind.turn-off", what = "pet")
                        ),
                    }
                }
            }
            _ => Delivery::Drop,
        },
        _ => Delivery::Drop,
    })
}

/// Sends one due reminder. True when a message went out.
async fn deliver(app: &App, r: &ReminderRow, now: DateTime<Utc>) -> anyhow::Result<bool> {
    let lang = delivery_lang(app, r).await;
    let (title, text) = match plan(app, r, now, lang).await? {
        Delivery::Send { title, text } => (title, text),
        Delivery::Drop => {
            app.db.finish_reminder(r).await?;
            return Ok(false);
        }
        Delivery::Later(at) => {
            app.db.postpone_reminder(r, at).await?;
            return Ok(false);
        }
    };
    let user = r.user_id as u64;
    let embed = EmbedBuilder::new().color(BRAND).title(title).description(text).build();
    let mentions = AllowedMentions::default();
    let sent = async {
        let channel = open_private_channel(app, user)
            .await
            .ok_or_else(|| anyhow::anyhow!("direct messages are closed"))?;
        app.http
            .create_message(channel)
            .embeds(&[embed])
            .allowed_mentions(Some(&mentions))
            .await?;
        anyhow::Ok(())
    }
    .await;
    // Closed direct messages are the player's choice: the reminder is dropped, not retried.
    if let Err(error) = &sent {
        tracing::debug!(%error, user, kind = %r.kind, "reminder not delivered");
    }
    app.db.finish_reminder(r).await?;
    Ok(sent.is_ok())
}

/// Sends every reminder that is due: one pass of the ticker. A batch goes out at once, and batches
/// follow each other while there are more and the pass has used less than [`BUDGET`]. Returns how
/// many messages went out.
pub async fn tick(app: &Arc<App>) -> anyhow::Result<usize> {
    let now = app.now();
    let started = Instant::now();
    let mut sent = 0;
    loop {
        let due = app.db.claim_due_reminders(now, BATCH).await?;
        let full = due.len() as i64 == BATCH;
        let mut batch = tokio::task::JoinSet::new();
        for reminder in due {
            let app = app.clone();
            batch.spawn(async move {
                let result = deliver(&app, &reminder, now).await;
                (reminder.id, result)
            });
        }
        while let Some(done) = batch.join_next().await {
            match done {
                Ok((_, Ok(true))) => sent += 1,
                Ok((_, Ok(false))) => {}
                // The lease runs out and the reminder is tried again.
                Ok((id, Err(error))) => tracing::warn!(%error, id, "reminder not handled"),
                Err(error) => tracing::warn!(%error, "reminder task failed"),
            }
        }
        if !full || started.elapsed() > BUDGET {
            break;
        }
    }
    metrics::counter!("pinchora_reminders_sent_total").increment(sent as u64);
    Ok(sent)
}

/// Runs [`tick`] every 30 seconds for the life of the process, and forgets the players who deleted
/// their data long enough ago once an hour.
pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut wake = tokio::time::interval(WAKE);
        wake.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut rounds = 0u32;
        loop {
            wake.tick().await;
            if let Err(error) = pinchora_services::infra::Job::new("reminders").run(tick(&app)).await {
                tracing::warn!(%error, "reminder tick failed");
            }
            rounds = rounds.wrapping_add(1);
            if rounds.is_multiple_of(FORGET_EVERY)
                && let Err(error) = app.db.forget_old_tombstones(app.now()).await
            {
                tracing::warn!(%error, "old deletions not forgotten");
            }
        }
    });
}

