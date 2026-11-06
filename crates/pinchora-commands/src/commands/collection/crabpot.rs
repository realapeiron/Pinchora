//! `/crabpot`: set a pot for one, four or eight hours, come back and haul in what it caught.

use std::{collections::HashMap, str::Split, sync::Arc};

use chrono::{Datelike, Duration, Timelike};
use pinchora_db::{Granted, PotHaul, PotRow, PotUpgrade};
use pinchora_game::{
    collection::{self, Collection, POT_HOURS, POT_MAX_LEVEL},
    play::Game,
};
use pinchora_render::{HaulCard, PotCard, PotState, Theme};
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{channel::message::component::ButtonStyle, http::interaction::InteractionResponseData};
use twilight_util::builder::embed::EmbedBuilder;

use super::{award, daily, dice, haul_tile, money};
use crate::{
    App, Cx,
    commands::games::record,
    server_events::{button, row},
    util::{BRAND, SUCCESS, thousands},
};

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "crabpot",
    desc = "Set a crab pot for a few hours, then haul in the critters it caught"
)]
pub struct CrabPot;

impl CrabPot {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let data = view(&cx.app, cx.author()?.id.get(), None, true).await?;
        cx.reply(data).await
    }
}

/// "3 h 20 m", "45 m", "under a minute".
fn span(left: Duration) -> String {
    let minutes = (left.num_seconds() + 59) / 60;
    match minutes {
        ..=0 => "under a minute".to_owned(),
        1..=59 => format!("{minutes} m"),
        _ if minutes % 60 == 0 => format!("{} h", minutes / 60),
        _ => format!("{} h {} m", minutes / 60, minutes % 60),
    }
}

/// What the pot is doing at `now`.
fn state(pot: &PotRow, now: chrono::DateTime<chrono::Utc>) -> PotState {
    match (pot.hours, pot.ends_at) {
        (Some(hours), Some(end)) if end > now => {
            let total = Duration::hours(i64::from(hours)).num_seconds() as f32;
            let left = (end - now).num_seconds() as f32;
            PotState::Soaking {
                left: span(end - now),
                fraction: (1.0 - left / total).clamp(0.0, 1.0),
                hours: hours as u32,
            }
        }
        (Some(hours), Some(_)) => PotState::Ready { hours: hours as u32 },
        _ => PotState::Empty,
    }
}

/// The pot card with the buttons that fit what it is doing; `note` goes in the text above it.
async fn view(
    app: &Arc<App>,
    user: u64,
    note: Option<&str>,
    ephemeral: bool,
) -> anyhow::Result<InteractionResponseData> {
    let pot = app.db.pot(user).await?;
    let state = state(&pot, app.now());
    let cost = collection::pot_upgrade_cost(pot.level);
    let card = PotCard {
        level: pot.level as u8,
        max_level: POT_MAX_LEVEL as u8,
        state: state.clone(),
        rate: format!("{:.1} catches an hour", collection::pot_rate(pot.level)),
        upgrade: cost.map_or(String::new(), |c| {
            format!("Level {} costs {} shells", pot.level + 1, thousands(c))
        }),
        theme: Theme::reef(),
    };
    let png = app.render(move |r| r.pot_card(&card)).await?;
    let mut buttons = match state {
        PotState::Empty => POT_HOURS
            .iter()
            .map(|h| {
                button(
                    format!("kp:s:{user}:{h}"),
                    &format!("Set for {h} h"),
                    Some("🪤"),
                    ButtonStyle::Primary,
                    false,
                )
            })
            .collect(),
        PotState::Soaking { .. } => vec![button(
            format!("kp:r:{user}"),
            "Check the tide",
            Some("🌊"),
            ButtonStyle::Secondary,
            false,
        )],
        PotState::Ready { .. } => vec![button(
            format!("kp:h:{user}"),
            "Haul it in!",
            Some("🦀"),
            ButtonStyle::Success,
            false,
        )],
    };
    if let Some(cost) = cost {
        buttons.push(button(
            format!("kp:u:{user}:{}", pot.level),
            &format!("Upgrade ({} shells)", thousands(cost)),
            Some("⬆️"),
            ButtonStyle::Secondary,
            false,
        ));
    }
    let status = match &state {
        PotState::Empty => "empty".to_owned(),
        PotState::Soaking { left, .. } => format!("soaking, {left} left"),
        PotState::Ready { .. } => "ready to haul".to_owned(),
    };
    let text = match note {
        Some(note) => format!("{note}\n\nCrab pot, level {}: {status}.", pot.level),
        None => format!("Crab pot, level {}: {status}.", pot.level),
    };
    Ok(Cx::card(
        EmbedBuilder::new().color(BRAND).description(text),
        "pot.png",
        png,
        vec![row(buttons)],
        ephemeral,
    ))
}

/// "1 catch", "10 catches".
fn catches(n: usize) -> String {
    format!("{n} catch{}", if n == 1 { "" } else { "es" })
}

/// One kind of critter or find in a haul.
struct Catch {
    entry: &'static pinchora_game::collection::Entry,
    count: i32,
    new: bool,
    shells: i64,
    fragments: i64,
}

/// The haul as a card: each kind once with how many of it, the rarest first.
fn haul_card(app: &App, granted: &[Granted], paid: bool, hours: i16) -> HaulCard {
    let mut kinds: HashMap<&str, Catch> = HashMap::new();
    for g in granted {
        let Some(entry) = Collection::get().entry(&g.entry) else {
            continue;
        };
        let kind = kinds.entry(&g.entry).or_insert(Catch {
            entry,
            count: 0,
            new: false,
            shells: 0,
            fragments: 0,
        });
        kind.count += 1;
        kind.new |= g.new;
        kind.shells += g.shells;
        kind.fragments += g.fragments;
    }
    let mut kinds: Vec<Catch> = kinds.into_values().collect();
    kinds.sort_by(|a, b| {
        b.entry
            .rarity
            .cmp(&a.entry.rarity)
            .then(a.entry.name.cmp(&b.entry.name))
    });
    let (shells, fragments): (i64, i64) = kinds
        .iter()
        .fold((0, 0), |sum, k| (sum.0 + k.shells, sum.1 + k.fragments));
    let more = kinds.len().saturating_sub(HaulCard::MAX_TILES);
    let tiles = kinds
        .iter()
        .take(HaulCard::MAX_TILES)
        .map(|k| {
            let size = if kinds.len() == 1 {
                HaulCard::SINGLE_ART_BOX
            } else {
                HaulCard::GRID_ART_BOX
            };
            let mut tile = haul_tile(app, k.entry, None, size);
            tile.tag = match (k.new, k.count) {
                (true, 1) => "NEW".to_owned(),
                (true, n) => format!("NEW x{n}"),
                (false, 1) => "SPARE".to_owned(),
                (false, n) => format!("SPARE x{n}"),
            };
            tile.line = money(&Granted {
                entry: String::new(),
                new: false,
                caught: 0,
                shells: k.shells,
                fragments: k.fragments,
            });
            tile
        })
        .collect();
    let mut parts = vec![catches(granted.len())];
    if shells > 0 {
        parts.push(format!("+{} shells", thousands(shells)));
    }
    if fragments > 0 {
        parts.push(format!("+{} fragments", thousands(fragments)));
    }
    if more > 0 {
        parts.push(format!("{more} more kind{}", if more == 1 { "" } else { "s" }));
    }
    let mut footer = parts.join(" · ");
    if !paid {
        footer = "Daily haul limit reached: the catch fills your dex, shells wait for tomorrow".to_owned();
    }
    HaulCard {
        title: "Crab pot".to_owned(),
        subtitle: format!("{hours} h soak").to_uppercase(),
        tiles,
        footer,
        theme: Theme::reef(),
    }
}

/// `kp:s:<owner>:<hours>` sets the pot, `kp:h:<owner>` hauls it in, `kp:r:<owner>` looks again and
/// `kp:u:<owner>:<level>` upgrades it.
pub(crate) async fn on_component(cx: &Cx, mut parts: Split<'_, char>) -> anyhow::Result<()> {
    let action = parts.next().unwrap_or_default();
    let owner: u64 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let (app, me) = (&cx.app, cx.author()?.id.get());
    if owner != me {
        return cx
            .reply_ephemeral("That is not your pot. Set your own with `/crabpot`.")
            .await;
    }
    let now = app.now();
    let note = match action {
        "r" => None,
        "s" => {
            let hours: i16 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            if !POT_HOURS.contains(&hours) {
                return cx.reply_ephemeral("This button has expired.").await;
            }
            if app.db.start_pot(me, hours, now).await? {
                Some(format!(
                    "The pot is in the water for {hours} h. Come back to haul it in."
                ))
            } else {
                Some("The pot is already in the water. Haul it in first.".to_owned())
            }
        }
        "u" => {
            let from: i16 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            let Some(cost) = collection::pot_upgrade_cost(from) else {
                return cx
                    .update(view(app, me, Some("Your pot is already the best there is."), false).await?)
                    .await;
            };
            Some(match app.db.upgrade_pot(me, from, cost).await? {
                PotUpgrade::Upgraded { level, balance } => format!(
                    "Your pot is now **level {level}**: it catches more, and luckier. You have {} shells left.",
                    thousands(balance)
                ),
                PotUpgrade::Stale => "Your pot is not at that level any more.".to_owned(),
                PotUpgrade::Poor { balance } => {
                    format!(
                        "An upgrade costs **{}** shells and you have {}.",
                        thousands(cost),
                        thousands(balance)
                    )
                }
            })
        }
        "h" => {
            let haul = app
                .db
                .haul_pot(me, now, daily(app, Game::CrabPot), |level, hours, ends_at| {
                    // The catches come evenly through the soak, each drawn for the hour it was made.
                    let (catches, soak) = (
                        collection::pot_catches(level, hours) as i32,
                        Duration::hours(hours.into()),
                    );
                    (0..catches)
                        .filter_map(|k| {
                            let at = ends_at - soak + soak * (2 * k + 1) / (2 * catches);
                            collection::pot_catch(level, at.hour(), at.month(), &mut dice())
                        })
                        .map(award)
                        .collect()
                })
                .await?;
            match haul {
                PotHaul::Empty => Some("Your pot is empty. Set it for 1, 4 or 8 hours.".to_owned()),
                PotHaul::Soaking { ends_at } => Some(format!("Still soaking: ready <t:{}:R>.", ends_at.timestamp())),
                PotHaul::Hauled {
                    granted, paid, hours, ..
                } => {
                    record(app, Game::CrabPot, me, true).await;
                    let card = haul_card(app, &granted, paid, hours);
                    let png = app.render(move |r| r.haul_card(&card)).await?;
                    let new = granted.iter().filter(|g| g.new).count();
                    let embed = EmbedBuilder::new().color(SUCCESS).description(format!(
                        "You hauled in **{}** catch{}{}.{}",
                        granted.len(),
                        if granted.len() == 1 { "" } else { "es" },
                        if new > 0 {
                            format!(", {new} new for your dex")
                        } else {
                            String::new()
                        },
                        if paid {
                            ""
                        } else {
                            " Daily haul limit reached: the catch is in your dex, but shells and fragments wait for tomorrow."
                        }
                    ));
                    let back = row(vec![button(
                        format!("kp:r:{me}"),
                        "Back to the pot",
                        None,
                        ButtonStyle::Secondary,
                        false,
                    )]);
                    return cx.update(Cx::card(embed, "haul.png", png, vec![back], false)).await;
                }
            }
        }
        _ => return cx.reply_ephemeral("This button has expired.").await,
    };
    cx.update(view(app, me, note.as_deref(), false).await?).await
}
