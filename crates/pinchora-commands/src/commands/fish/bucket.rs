//! `/fish sell`: the fish bucket and selling what is in it.

use std::str::Split;

use pinchora_db::BucketRow;
use pinchora_game::{Content, Fish, fishing};
use twilight_model::{
    channel::message::{
        Component, MessageFlags,
        component::{ActionRow, Button, ButtonStyle},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::EmbedBuilder;

use super::art::{Art, Slot, grid_png};
use crate::{
    App, Cx,
    commands::customize,
    util::{BRAND, thousands},
};

const PER_PAGE: usize = 8;

/// The fish of one species in the bucket, the biggest first.
struct Group {
    fish: &'static Fish,
    rows: Vec<BucketRow>,
}

impl Group {
    fn value(&self) -> i64 {
        self.rows
            .iter()
            .map(|r| fishing::sale_value(self.fish, r.size_cm))
            .sum()
    }
}

/// What a sale takes out of the bucket.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Every fish.
    All,
    /// All but the biggest of each species.
    Duplicates,
}

impl Mode {
    fn key(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Duplicates => "dup",
        }
    }
}

/// The bucket by species, the most valuable first. Rows come from the database species by species,
/// the biggest first within each.
fn groups(rows: Vec<BucketRow>) -> Vec<Group> {
    let content = Content::get();
    let mut groups: Vec<Group> = Vec::new();
    for row in rows {
        let Some(fish) = content.fish(&row.fish_id) else {
            continue;
        };
        match groups.last_mut() {
            Some(group) if group.fish.id == fish.id => group.rows.push(row),
            _ => groups.push(Group { fish, rows: vec![row] }),
        }
    }
    groups.sort_by_key(|g| std::cmp::Reverse((g.fish.rarity, g.value())));
    groups
}

/// The (fish, shells) pairs a sale of `mode` would sell.
fn sales(groups: &[Group], mode: Mode) -> Vec<(i64, i64)> {
    groups
        .iter()
        .flat_map(|g| {
            let skip = usize::from(mode == Mode::Duplicates);
            g.rows
                .iter()
                .skip(skip)
                .map(|r| (r.id, fishing::sale_value(g.fish, r.size_cm)))
        })
        .collect()
}

fn total(sales: &[(i64, i64)]) -> i64 {
    sales.iter().map(|(_, value)| value).sum()
}

pub(super) async fn sell(cx: &Cx) -> anyhow::Result<()> {
    let data = panel(&cx.app, cx.author()?.id.get(), 0, "").await?;
    cx.reply(data).await
}

async fn panel(app: &App, user: u64, page: usize, note: &str) -> anyhow::Result<InteractionResponseData> {
    let groups = groups(app.db.bucket(user).await?);
    let count: usize = groups.iter().map(|g| g.rows.len()).sum();
    let pages = groups.len().div_ceil(PER_PAGE).max(1);
    let page = page.min(pages - 1);
    let (all, duplicates) = (sales(&groups, Mode::All), sales(&groups, Mode::Duplicates));

    let slots = groups
        .iter()
        .skip(page * PER_PAGE)
        .take(PER_PAGE)
        .map(|g| Slot {
            art: Art::Fish {
                id: g.fish.id.clone(),
                hidden: false,
            },
            name: g.fish.name.clone(),
            line1: thousands(g.value()),
            line2: format!("×{} · BIGGEST {:.1} CM", g.rows.len(), g.rows[0].size_cm),
            bar: None,
            frame: g.fish.rarity.color(),
            badge: g.fish.rarity.label().to_uppercase(),
            shell_price: true,
        })
        .collect();
    let png = grid_png(
        app,
        "Fish bucket".to_owned(),
        format!("{count}/{} FISH · PAGE {}/{pages}", fishing::BUCKET_CAPACITY, page + 1),
        format!("EVERYTHING SELLS FOR {} SHELLS", thousands(total(&all))),
        slots,
    )
    .await?;
    let description = if note.is_empty() {
        if count == 0 {
            "The bucket is empty. Cast a line with `/fish cast`!".to_owned()
        } else {
            "Sell it all, or keep the biggest of each kind and sell the duplicates. \
             Your fish book keeps every species and record size for good."
                .to_owned()
        }
    } else {
        note.to_owned()
    };
    let button = |custom_id: String, label: String, style: ButtonStyle, disabled: bool| {
        Component::Button(Button {
            custom_id: Some(custom_id),
            disabled,
            emoji: None,
            id: None,
            label: Some(label),
            sku_id: None,
            style,
            url: None,
        })
    };
    let components = vec![Component::ActionRow(ActionRow {
        id: None,
        components: vec![
            button(
                format!("fz:k:{user}:{}", page.saturating_sub(1)),
                "◀".to_owned(),
                ButtonStyle::Secondary,
                page == 0,
            ),
            button(
                format!("fz:k:{user}:{}", page + 1),
                "▶".to_owned(),
                ButtonStyle::Secondary,
                page + 1 >= pages,
            ),
            button(
                format!("fz:s:{user}:{}", Mode::All.key()),
                format!("Sell all ({} · {} shells)", all.len(), thousands(total(&all))),
                ButtonStyle::Success,
                all.is_empty(),
            ),
            button(
                format!("fz:s:{user}:{}", Mode::Duplicates.key()),
                format!(
                    "Sell duplicates ({} · {} shells)",
                    duplicates.len(),
                    thousands(total(&duplicates))
                ),
                ButtonStyle::Primary,
                duplicates.is_empty(),
            ),
        ],
    })];
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title("🪣 Your fish bucket")
        .description(description);
    let mut data = Cx::card(embed, "bucket.png", png, components, true);
    data.flags = Some(MessageFlags::EPHEMERAL);
    Ok(data)
}

/// `fz:k:<user>:<page>` pages the bucket, `fz:s:<user>:<all|dup>` sells; `fz:b:` is the book's.
pub(crate) async fn on_bucket(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let what = args.next().unwrap_or_default();
    if what == "b" {
        return super::book::on_page(cx, args).await;
    }
    let owner: u64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let user = cx.author()?.id.get();
    if user != owner {
        return cx
            .reply_ephemeral("That's someone else's bucket. Open yours with `/fish sell`.")
            .await;
    }
    match what {
        "k" => {
            let page: usize = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            cx.update(panel(&cx.app, user, page, "").await?).await
        }
        "s" => {
            let mode = if args.next() == Some(Mode::Duplicates.key()) {
                Mode::Duplicates
            } else {
                Mode::All
            };
            let groups = groups(cx.app.db.bucket(user).await?);
            let sales = sales(&groups, mode);
            let note = if sales.is_empty() {
                "Nothing to sell right now.".to_owned()
            } else {
                let sold = cx.app.db.sell_fish(user, &sales).await?;
                format!(
                    "Sold **{} fish** for **{} shells**. You have {}.",
                    sold.fish,
                    thousands(sold.shells),
                    thousands(sold.balance)
                )
            };
            cx.update(panel(&cx.app, user, 0, &note).await?).await?;
            // The shells may have just earned a badge.
            customize::announce(cx).await;
            Ok(())
        }
        _ => cx.reply_ephemeral("This button has expired.").await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: i64, fish: &str, size: f32) -> BucketRow {
        BucketRow {
            id,
            fish_id: fish.to_owned(),
            size_cm: size,
        }
    }

    #[test]
    fn duplicates_keep_the_biggest_of_each_species() {
        // The database hands them species by species, the biggest first.
        let rows = vec![
            row(1, "pearlpatch_koi", 60.0),
            row(2, "pearlpatch_koi", 40.0),
            row(3, "pearlpatch_koi", 30.0),
            row(4, "toaststripe_minnow", 6.0),
        ];
        let groups = groups(rows);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].fish.id, "pearlpatch_koi", "the dearer species leads");
        let all = sales(&groups, Mode::All);
        assert_eq!(all.len(), 4);
        let duplicates = sales(&groups, Mode::Duplicates);
        assert_eq!(duplicates.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [2, 3]);
        assert!(total(&duplicates) < total(&all));
        assert_eq!(groups[0].value(), all[..3].iter().map(|(_, v)| v).sum::<i64>());
        // A bigger fish sells for more.
        assert!(all[0].1 > all[1].1 && all[1].1 > all[2].1);
    }

    #[test]
    fn fish_the_game_no_longer_knows_are_left_in_the_bucket() {
        assert!(groups(vec![row(1, "not_a_fish", 10.0)]).is_empty());
    }
}
