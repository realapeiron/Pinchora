//! `/fish sell`: the fish bucket and selling what is in it.

use std::str::Split;

use pinchora_core::{i18n::Lang, t};
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

/// The bucket as the game sees it: fish by species, plus any left of a species the game no longer has.
struct Bucket {
    /// By species, the most valuable first.
    groups: Vec<Group>,
    /// Fish of species taken out of the game: they count for nothing and sell for nothing, but
    /// Sell all clears them away.
    strays: Vec<BucketRow>,
}

impl Bucket {
    /// Fish of known species, which is what the bucket's size is counted in.
    fn count(&self) -> usize {
        self.groups.iter().map(|g| g.rows.len()).sum()
    }
}

/// Sorts the bucket's rows into species, the most valuable first. Rows come from the database species
/// by species, the biggest first within each.
fn bucket_of(rows: Vec<BucketRow>) -> Bucket {
    let content = Content::get();
    let mut groups: Vec<Group> = Vec::new();
    let mut strays = Vec::new();
    for row in rows {
        let Some(fish) = content.fish(&row.fish_id) else {
            strays.push(row);
            continue;
        };
        match groups.last_mut() {
            Some(group) if group.fish.id == fish.id => group.rows.push(row),
            _ => groups.push(Group { fish, rows: vec![row] }),
        }
    }
    groups.sort_by_key(|g| std::cmp::Reverse((g.fish.rarity, g.value())));
    Bucket { groups, strays }
}

/// The (fish, shells) pairs a sale of `mode` would sell. Selling everything also clears the strays,
/// for nothing.
fn sales(bucket: &Bucket, mode: Mode) -> Vec<(i64, i64)> {
    let mut sales: Vec<(i64, i64)> = bucket
        .groups
        .iter()
        .flat_map(|g| {
            let skip = usize::from(mode == Mode::Duplicates);
            g.rows
                .iter()
                .skip(skip)
                .map(|r| (r.id, fishing::sale_value(g.fish, r.size_cm)))
        })
        .collect();
    if mode == Mode::All {
        sales.extend(bucket.strays.iter().map(|r| (r.id, 0)));
    }
    sales
}

fn total(sales: &[(i64, i64)]) -> i64 {
    sales.iter().map(|(_, value)| value).sum()
}

pub(super) async fn sell(cx: &Cx) -> anyhow::Result<()> {
    let data = panel(&cx.app, cx.lang(), cx.author()?.id.get(), 0, "").await?;
    cx.reply(data).await
}

async fn panel(app: &App, lang: Lang, user: u64, page: usize, note: &str) -> anyhow::Result<InteractionResponseData> {
    let content = Content::get();
    let bucket = bucket_of(app.db.bucket(user).await?);
    let count = bucket.count();
    let pages = bucket.groups.len().div_ceil(PER_PAGE).max(1);
    let page = page.min(pages - 1);
    let (all, duplicates) = (sales(&bucket, Mode::All), sales(&bucket, Mode::Duplicates));

    let slots = bucket
        .groups
        .iter()
        .skip(page * PER_PAGE)
        .take(PER_PAGE)
        .map(|g| Slot {
            art: Art::Fish {
                id: g.fish.id.clone(),
                hidden: false,
            },
            name: content.name(&g.fish.id, lang).into_owned(),
            line1: thousands(g.value()),
            line2: t!(
                lang,
                "fish.bucket.slot",
                fish = g.rows.len(),
                size = format!("{:.1}", g.rows[0].size_cm)
            ),
            bar: None,
            frame: g.fish.rarity.color(),
            badge: lang.upper(g.fish.rarity.label_in(lang)),
            shell_price: true,
        })
        .collect();
    let png = grid_png(
        app,
        lang.upper(&t!(lang, "fish.bucket.title")),
        t!(
            lang,
            "fish.bucket.subtitle",
            fish = count,
            capacity = fishing::BUCKET_CAPACITY,
            page = page + 1,
            pages = pages
        ),
        t!(lang, "fish.bucket.footer", shells = thousands(total(&all))),
        slots,
    )
    .await?;
    let description = if note.is_empty() {
        if count == 0 && !bucket.strays.is_empty() {
            t!(lang, "fish.bucket.only-strays")
        } else if count == 0 {
            t!(lang, "fish.bucket.empty")
        } else {
            t!(lang, "fish.bucket.intro")
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
                t!(
                    lang,
                    "fish.bucket.sell-all",
                    fish = all.len(),
                    shells = thousands(total(&all))
                ),
                ButtonStyle::Success,
                all.is_empty(),
            ),
            button(
                format!("fz:s:{user}:{}", Mode::Duplicates.key()),
                t!(
                    lang,
                    "fish.bucket.sell-duplicates",
                    fish = duplicates.len(),
                    shells = thousands(total(&duplicates))
                ),
                ButtonStyle::Primary,
                duplicates.is_empty(),
            ),
        ],
    })];
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(lang, "fish.bucket.embed-title"))
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
    let lang = cx.lang();
    if user != owner {
        return cx.reply_ephemeral(t!(lang, "fish.bucket.not-yours")).await;
    }
    match what {
        "k" => {
            let page: usize = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            cx.update(panel(&cx.app, lang, user, page, "").await?).await
        }
        "s" => {
            let mode = if args.next() == Some(Mode::Duplicates.key()) {
                Mode::Duplicates
            } else {
                Mode::All
            };
            let bucket = bucket_of(cx.app.db.bucket(user).await?);
            let sales = sales(&bucket, mode);
            let note = if sales.is_empty() {
                t!(lang, "fish.bucket.nothing")
            } else {
                let sold = cx.app.db.sell_fish(user, &sales).await?;
                t!(
                    lang,
                    "fish.bucket.sold",
                    fish = sold.fish,
                    shells = thousands(sold.shells),
                    balance = thousands(sold.balance)
                )
            };
            cx.update(panel(&cx.app, lang, user, 0, &note).await?).await?;
            // The shells may have just earned a badge.
            customize::announce(cx).await;
            Ok(())
        }
        _ => cx.reply_ephemeral(t!(lang, "fish.bucket.expired")).await,
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
        let bucket = bucket_of(rows);
        let groups = &bucket.groups;
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].fish.id, "pearlpatch_koi", "the dearer species leads");
        let all = sales(&bucket, Mode::All);
        assert_eq!(all.len(), 4);
        let duplicates = sales(&bucket, Mode::Duplicates);
        assert_eq!(duplicates.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [2, 3]);
        assert!(total(&duplicates) < total(&all));
        assert_eq!(groups[0].value(), all[..3].iter().map(|(_, v)| v).sum::<i64>());
        // A bigger fish sells for more.
        assert!(all[0].1 > all[1].1 && all[1].1 > all[2].1);
    }

    #[test]
    fn fish_the_game_no_longer_knows_do_not_count_and_sell_all_clears_them_for_nothing() {
        let bucket = bucket_of(vec![
            row(1, "not_a_fish", 10.0),
            row(2, "pearlpatch_koi", 40.0),
            row(3, "gone_too", 3.0),
            row(4, "pearlpatch_koi", 30.0),
        ]);
        assert!(
            bucket.groups.len() == 1 && bucket.count() == 2,
            "only known species count"
        );
        assert_eq!(bucket.strays.iter().map(|r| r.id).collect::<Vec<_>>(), [1, 3]);
        // Sell all takes the strays along at 0 shells, so their total is just the known fish's.
        let all = sales(&bucket, Mode::All);
        assert_eq!(all.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [2, 4, 1, 3]);
        assert_eq!(all[2..], [(1, 0), (3, 0)]);
        assert_eq!(total(&all), bucket.groups[0].value());
        // Selling duplicates leaves them where they are.
        assert_eq!(
            sales(&bucket, Mode::Duplicates)
                .iter()
                .map(|(id, _)| *id)
                .collect::<Vec<_>>(),
            [4]
        );
        // A bucket of nothing but strays is empty to the count and still has something to clear.
        let only = bucket_of(vec![row(9, "not_a_fish", 1.0)]);
        assert_eq!((only.count(), only.groups.len()), (0, 0));
        assert_eq!(sales(&only, Mode::All), [(9, 0)]);
        assert!(sales(&only, Mode::Duplicates).is_empty());
    }
}
