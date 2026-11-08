//! `/help`: a guide to every command, drawn as pixel cards one area at a time. The pages are built
//! from the registered command tree, so a command added later shows up in its area by itself; a unit
//! test fails until a new command is given one.

use std::{collections::HashMap, str::Split, sync::LazyLock};

use pinchora_core::{i18n::Lang, t};
use pinchora_render::{HelpCard, HelpRow, Theme};
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    application::command::{Command, CommandOption, CommandOptionType},
    channel::message::{
        Component, MessageFlags,
        component::{ActionRow, Button, ButtonStyle, SelectMenu, SelectMenuOption, SelectMenuType},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFooterBuilder};

use crate::{Cx, util::BRAND};

#[derive(CommandModel, CreateCommand)]
#[command(name = "help", desc = "What Pinchora can do")]
pub struct Help;

impl Help {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        // Asked by text, the guide is posted for the channel: there is no private message to turn.
        let data = page_card(cx, 0, !cx.responder.is_text()).await?;
        cx.reply(data).await
    }
}

/// An area of the guide: its name, a line about it, and the commands it holds, as the start of their
/// path (`house` holds `/house view` and everything else under `/house`; `play stats` just that one).
pub(crate) struct Area {
    /// Its name and line are `help.area.<key>.title` and `.intro` in the catalogs.
    key: &'static str,
    pub(crate) covers: &'static [&'static str],
}

impl Area {
    pub(crate) fn title(&self, lang: Lang) -> String {
        t!(lang, &format!("help.area.{}.title", self.key))
    }

    pub(crate) fn intro(&self, lang: Lang) -> String {
        t!(lang, &format!("help.area.{}.intro", self.key))
    }
}

/// The areas in the order of the guide. A command belongs to the area with the longest start of its
/// path that matches, so `play stats` can sit with the profile while the other `/play` games sit elsewhere.
pub(crate) const AREAS: &[Area] = &[
    Area {
        key: "start",
        covers: &[
            "profile",
            "rank",
            "leaderboard",
            "daily",
            "balance",
            "quests",
            "help",
            "language",
            "ping",
        ],
    },
    Area {
        key: "profile",
        covers: &[
            "bio",
            "character",
            "customize",
            "wrapped",
            "privacy",
            "roblox",
            "robloxadmin",
            "play stats",
            "achievements",
            "achievementadmin",
        ],
    },
    Area {
        key: "rewards",
        covers: &["economyv2", "weekly", "monthly", "give", "rep", "remind", "birthday"],
    },
    Area {
        key: "shops",
        covers: &["shop", "inventory", "gift", "stall"],
    },
    Area {
        key: "house",
        covers: &["house", "plaza"],
    },
    Area {
        key: "pets",
        covers: &["pet"],
    },
    Area {
        key: "fishing",
        covers: &["fish"],
    },
    Area {
        key: "friends",
        covers: &["friends", "partner", "hug", "pat", "wave", "highfive"],
    },
    Area {
        key: "duels",
        covers: &["duel"],
    },
    Area {
        key: "puzzles",
        covers: &[
            "game",
            "play hangman",
            "play wordle",
            "play trivia",
            "play picross",
            "play 2048",
            "play memory",
            "play minesweeper",
            "play higherlower",
            "play anagram",
            "play typing",
            "play guessnumber",
            "play reefmind",
            "play kelpmaze",
            "play gemtide",
            "party",
        ],
    },
    Area {
        key: "crowd",
        covers: &[
            "play derby",
            "play heist",
            "play shells",
            "surge",
            "beachcomb",
            "crabpot",
            "critterdrop",
            "dex",
        ],
    },
    Area {
        key: "community",
        covers: &[
            "poll",
            "suggest",
            "suggestion",
            "giveaway",
            "highlight",
            "checklist",
            "autopost",
            "anniversary",
            "confess",
            "roles",
            "points",
            "store",
            "tag",
            "customcommand",
            "workflow",
            "voice",
            "feeds",
            "trackers",
            "autoroles",
            "color",
            "stats",
            "event",
            "qotd",
            "counting",
            "form",
            "modmail",
            "community qotd",
            "community counting",
            "community forms",
            "community modmail",
            "ticket",
            "tickets",
            "starboard",
            "invites",
            "inviteadmin",
            "afk",
            "sticky",
            "schedule",
        ],
    },
    Area {
        key: "moderation",
        covers: &[
            "automod",
            "commandcontrol",
            "backup",
            "branding",
            "verify",
            "verification",
            "raid",
            "antinuke status",
            "antinuke events",
            "modtools",
            "Report",
            "ban",
            "unban",
            "kick",
            "timeout",
            "untimeout",
            "warn",
            "warnings",
            "unwarn",
            "purge",
            "slowmode",
            "lock",
            "unlock",
            "nick",
            "role",
            "case",
            "reason",
        ],
    },
    Area {
        key: "setup",
        covers: &[
            "config show",
            "config prefix",
            "config welcome",
            "config welcome-card",
            "config leveling",
            "config join-role",
            "config goodbye",
            "config suggestions",
            "config language",
            "info",
            "counters",
        ],
    },
    Area {
        key: "logsetup",
        covers: &[
            "antinuke",
            "config log",
            "config log-events",
            "config log-ignore",
            "config moderation",
            "config warn-ladder",
        ],
    },
    Area {
        key: "levels",
        covers: &[
            "config level-roles",
            "config xp",
            "config rep-roles",
            "config points",
            "config store",
            "xp",
        ],
    },
    Area {
        key: "modules",
        covers: &[
            "config critters",
            "config gambling",
            "config surge",
            "config partygames",
            "config modules",
            "birthdays",
        ],
    },
];

/// Words for commands whose own description does not read well on a page; everything else says what
/// the command's description says.
const WORDS: &[(&str, &str)] = &[];

/// How much taller than the even share a page may get before the next row starts a new one: about a row.
const EVEN_SLACK: f32 = 30.0;

/// One page of the guide.
struct Page {
    title: String,
    intro: String,
    rows: Vec<HelpRow>,
}

/// A command that can be run: the path of names down to it, and what it says about itself.
pub(crate) struct Leaf {
    pub(crate) path: String,
    pub(crate) text: String,
    pub(crate) manager: bool,
    /// What it takes, in the order Discord shows the options.
    pub(crate) options: Vec<CommandOption>,
}

/// `description` in `lang`, from the localizations registration filled in.
fn described(description: &str, localizations: Option<&HashMap<String, String>>, lang: Lang) -> String {
    lang.discord_locales()
        .first()
        .and_then(|locale| localizations?.get(*locale))
        .map_or(description, String::as_str)
        .to_owned()
}

/// Every runnable command and subcommand of `commands`, in registration order, described in `lang`.
pub(crate) fn leaves(commands: &[Command], lang: Lang) -> Vec<Leaf> {
    fn walk(
        options: &[CommandOption],
        path: &str,
        text: (&str, String),
        manager: bool,
        lang: Lang,
        out: &mut Vec<Leaf>,
    ) {
        let subs: Vec<&CommandOption> = options
            .iter()
            .filter(|o| {
                matches!(
                    o.kind,
                    CommandOptionType::SubCommand | CommandOptionType::SubCommandGroup
                )
            })
            .collect();
        if subs.is_empty() {
            let (english, text) = text;
            let text = WORDS
                .iter()
                .find(|(p, _)| *p == path && lang == Lang::En)
                .map_or(text, |(_, words)| (*words).to_owned());
            out.push(Leaf {
                path: path.to_owned(),
                manager: manager || crate::routing::is_admin_path(path) || english.contains("Manage Server"),
                text,
                options: options.to_vec(),
            });
        }
        for sub in subs {
            walk(
                sub.options.as_deref().unwrap_or_default(),
                &format!("{path} {}", sub.name),
                (
                    &sub.description,
                    described(&sub.description, sub.description_localizations.as_ref(), lang),
                ),
                manager,
                lang,
                out,
            );
        }
    }
    let mut out = Vec::new();
    for command in commands {
        if command.kind == twilight_model::application::command::CommandType::Message {
            out.push(Leaf {
                path: command.name.clone(),
                text: pinchora_core::t!(lang, "parity.context-help"),
                manager: false,
                options: Vec::new(),
            });
            continue;
        }
        walk(
            &command.options,
            &command.name,
            (
                &command.description,
                described(&command.description, command.description_localizations.as_ref(), lang),
            ),
            command.default_member_permissions.is_some(),
            lang,
            &mut out,
        );
    }
    if out.iter().any(|leaf| leaf.path.starts_with("mod ")) {
        let original = crate::commands::legacy_definitions();
        for leaf in out.iter_mut().filter(|leaf| leaf.path.starts_with("mod ")) {
            let root = crate::routing::legacy(&leaf.path).split(' ').next().unwrap();
            leaf.manager = original
                .iter()
                .find(|c| c.name == root)
                .unwrap()
                .default_member_permissions
                .is_some_and(|p| p.contains(twilight_model::guild::Permissions::MANAGE_GUILD));
        }
    }
    out
}

/// The area that holds `path`: the one with the longest start of the path, if exactly one has it.
pub(crate) fn area_of(path: &str) -> Option<usize> {
    let path = crate::routing::legacy(path);
    let mut best: Option<(usize, usize)> = None;
    let mut tied = false;
    for (index, area) in AREAS.iter().enumerate() {
        for cover in area.covers {
            if path != *cover && !path.strip_prefix(cover).is_some_and(|rest| rest.starts_with(' ')) {
                continue;
            }
            match best {
                Some((_, longest)) if cover.len() < longest => {}
                Some((_, longest)) if cover.len() == longest => tied = true,
                _ => {
                    best = Some((index, cover.len()));
                    tied = false;
                }
            }
        }
    }
    best.filter(|_| !tied).map(|(index, _)| index)
}

/// The pages: each area's commands in the order its `covers` list names them, started over on a new
/// page whenever the rows would not fit one card. Anything no area claims lands on a last page, so
/// the guide never leaves a command out (a test keeps that page empty).
fn build_legacy(commands: &[Command], lang: Lang) -> Vec<Page> {
    // Append each rollout cohort. Both original and slices 1–6 hp:<page-index> destinations
    // must remain stable when later groups introduce more rows in the same help area.
    let mut pages = build_pages(&crate::commands::legacy_definitions(), lang);
    let (mut early, late): (Vec<_>, Vec<_>) = leaves(commands, lang)
        .into_iter()
        .filter(|leaf| crate::routing::route(&leaf.path).is_some_and(|r| r.active() && r.canonical == leaf.path))
        .partition(|leaf| crate::routing::route(&leaf.path).unwrap().slice <= 6);
    let order = [
        "server",
        "welcome",
        "moderation",
        "leveling",
        "economy",
        "roles",
        "tickets",
        "utility",
        "automod",
        "verification",
        "antinuke",
    ];
    early.sort_by_key(|leaf| {
        order
            .iter()
            .position(|g| Some(*g) == leaf.path.split(' ').nth(1))
            .unwrap()
    });
    pages.extend(build_leaf_pages(early, lang));
    pages.extend(build_leaf_pages(late, lang));
    pages
}

fn build(commands: &[Command], lang: Lang) -> Vec<Page> {
    build_pages(commands, lang)
}

/// Old hp:<index> messages predate compact pages. Keep their area/action destination.
fn legacy_page(at: usize, lang: Lang, pages: &[Page]) -> usize {
    static LEGACY: LazyLock<HashMap<Lang, Vec<Page>>> = LazyLock::new(|| {
        let mut previous = crate::commands::legacy_definitions();
        crate::routing::extend(&mut previous);
        Lang::ALL
            .into_iter()
            .map(|lang| (lang, build_legacy(&previous, lang)))
            .collect()
    });
    let old = &LEGACY[&lang];
    old.get(at)
        .and_then(|page| page.rows.first())
        .and_then(|row| {
            let canonical = crate::routing::canonical(row.command.trim_start_matches('/'));
            pages
                .iter()
                .position(|p| p.rows.iter().any(|r| r.command == format!("/{canonical}")))
        })
        .unwrap_or(0)
}

fn build_pages(commands: &[Command], lang: Lang) -> Vec<Page> {
    build_leaf_pages(leaves(commands, lang), lang)
}

fn build_leaf_pages(leaves: Vec<Leaf>, lang: Lang) -> Vec<Page> {
    let mut by_area: Vec<Vec<(usize, HelpRow)>> = (0..=AREAS.len()).map(|_| Vec::new()).collect();
    for leaf in leaves {
        let area = area_of(&leaf.path);
        let order = area.map_or(0, |a| {
            AREAS[a]
                .covers
                .iter()
                .position(|c| {
                    crate::routing::legacy(&leaf.path) == *c
                        || crate::routing::legacy(&leaf.path).starts_with(&format!("{c} "))
                })
                .unwrap_or(0)
        });
        by_area[area.unwrap_or(AREAS.len())].push((
            order,
            HelpRow {
                command: format!("/{}", leaf.path),
                text: leaf.text,
                manager: leaf.manager,
            },
        ));
    }
    let mut pages = Vec::new();
    for (index, mut rows) in by_area.into_iter().enumerate() {
        if rows.is_empty() {
            continue;
        }
        // Stable: the commands under one start of a path keep the order they were registered in.
        rows.sort_by_key(|(order, _)| *order);
        let (title, intro) = AREAS.get(index).map_or_else(
            || (t!(lang, "help.area.more.title"), t!(lang, "help.area.more.intro")),
            |a| (a.title(lang), a.intro(lang)),
        );
        // As many pages as the rows need, filled evenly: a page of fourteen rows and one of two reads worse
        // than two of eight.
        let total: f32 = rows.iter().map(|(_, row)| HelpCard::row_height(row)).sum();
        let wanted = (total / HelpCard::BODY_MAX).ceil().max(1.0);
        let limit = (total / wanted + EVEN_SLACK).min(HelpCard::BODY_MAX);
        let mut chunks: Vec<Vec<HelpRow>> = vec![Vec::new()];
        let mut height = 0.0;
        for (_, row) in rows {
            let tall = HelpCard::row_height(&row);
            if height + tall > limit && !chunks.last().is_some_and(Vec::is_empty) {
                chunks.push(Vec::new());
                height = 0.0;
            }
            height += tall;
            chunks.last_mut().expect("a chunk").push(row);
        }
        let count = chunks.len();
        for (n, rows) in chunks.into_iter().enumerate() {
            pages.push(Page {
                title: if count > 1 {
                    t!(lang, "help.page-of", title = title, page = n + 1, pages = count)
                } else {
                    title.clone()
                },
                intro: intro.clone(),
                rows,
            });
        }
    }
    pages
}

/// The guide in every language, built once.
static PAGES: LazyLock<HashMap<Lang, Vec<Page>>> = LazyLock::new(|| {
    let commands = crate::command_definitions();
    Lang::ALL
        .into_iter()
        .map(|lang| (lang, build(&commands, lang)))
        .collect()
});

fn button(page: usize, label: &str, disabled: bool) -> Component {
    Component::Button(Button {
        custom_id: Some(format!("hp:v2:{page}")),
        disabled,
        emoji: None,
        id: None,
        label: Some(label.to_owned()),
        sku_id: None,
        style: ButtonStyle::Secondary,
        url: None,
    })
}

/// What a menu entry says about its page: the first few commands on it.
fn preview(page: &Page) -> String {
    let names: Vec<&str> = page.rows.iter().map(|r| r.command.as_str()).collect();
    let mut text = names.iter().take(4).copied().collect::<Vec<_>>().join(" ");
    if names.len() > 4 {
        text.push_str(" …");
    }
    text.chars().take(100).collect()
}

/// Up to 25 nearby pages fit Discord's menu; the buttons still reach the whole guide.
fn controls(pages: &[Page], at: usize, lang: Lang) -> Vec<Component> {
    let start = at.saturating_sub(12).min(pages.len().saturating_sub(25));
    let options = pages
        .iter()
        .enumerate()
        .skip(start)
        .take(25)
        .map(|(n, page)| SelectMenuOption {
            default: n == at,
            description: Some(preview(page)),
            emoji: None,
            label: page.title.clone(),
            value: n.to_string(),
        })
        .collect();
    vec![
        Component::ActionRow(ActionRow {
            id: None,
            components: vec![Component::SelectMenu(SelectMenu {
                channel_types: None,
                custom_id: "hp:v2:s".to_owned(),
                default_values: None,
                disabled: false,
                id: None,
                kind: SelectMenuType::Text,
                max_values: Some(1),
                min_values: Some(1),
                options: Some(options),
                placeholder: Some(t!(lang, "help.jump")),
                required: None,
            })],
        }),
        Component::ActionRow(ActionRow {
            id: None,
            components: vec![
                button(at.saturating_sub(1), &t!(lang, "help.previous"), at == 0),
                button(at + 1, &t!(lang, "help.next"), at + 1 >= pages.len()),
            ],
        }),
    ]
}

/// The card for page `at` (the last one when it is past the end), with its controls. Private, the
/// controls are one person's to turn.
async fn page_card(cx: &Cx, at: usize, private: bool) -> anyhow::Result<InteractionResponseData> {
    let lang = cx.lang();
    let pages = &PAGES[&lang];
    let at = at.min(pages.len().saturating_sub(1));
    let page = &pages[at];
    let card = HelpCard {
        lang,
        title: page.title.clone(),
        intro: page.intro.clone(),
        page: at + 1,
        pages: pages.len(),
        rows: page.rows.clone(),
        theme: Theme::reef(),
    };
    let png = cx.app.render(move |r| r.help_card(&card)).await?;
    let mut description = t!(lang, "help.description");
    if let Some(guild) = cx.guild_id() {
        let prefix = crate::text::shown_prefix(&cx.app, guild).await;
        description.push('\n');
        description.push_str(&t!(lang, "text.help-hint", prefix = prefix));
    }
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(lang, "help.title"))
        .description(description)
        .footer(EmbedFooterBuilder::new(t!(lang, "help.footer")));
    Ok(Cx::card(embed, "help.png", png, controls(pages, at, lang), private))
}

/// `hp:<page>` buttons and the `hp:s` menu, whose value is the page.
pub(crate) async fn on_component(cx: &Cx, mut args: Split<'_, char>, value: Option<&str>) -> anyhow::Result<()> {
    let first = args.next();
    let modern = first == Some("v2");
    let wanted = match if modern { args.next() } else { first } {
        Some("s") => value,
        other => other,
    };
    let at = wanted.and_then(|p| p.parse().ok()).unwrap_or(0);
    let at = if modern {
        at
    } else {
        legacy_page(at, cx.lang(), &PAGES[&cx.lang()])
    };
    let private = cx
        .interaction
        .message
        .as_ref()
        .and_then(|m| m.flags)
        .is_none_or(|flags| flags.contains(MessageFlags::EPHEMERAL));
    let data = page_card(cx, at, private).await?;
    cx.update(data).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> Vec<Command> {
        crate::command_definitions()
    }

    /// The rows of the pages that are for `path`.
    fn rows_of<'a>(pages: &'a [Page], path: &str) -> Vec<&'a HelpRow> {
        let command = format!("/{path}");
        pages
            .iter()
            .flat_map(|p| &p.rows)
            .filter(|r| r.command == command)
            .collect()
    }

    #[test]
    fn every_command_and_subcommand_is_on_exactly_one_page() {
        let tree = tree();
        let pages = build(&tree, Lang::En);
        let all = leaves(&tree, Lang::En);
        assert!(all.len() > 100, "the walk found only {} commands", all.len());
        let mut missing = Vec::new();
        for leaf in &all {
            match rows_of(&pages, &leaf.path).len() {
                1 => {}
                0 => missing.push(format!("/{} is on no page", leaf.path)),
                n => missing.push(format!("/{} is on {n} pages", leaf.path)),
            }
        }
        assert!(
            missing.is_empty(),
            "give these commands an area in AREAS:\n{}",
            missing.join("\n")
        );
        let rows: usize = pages.iter().map(|p| p.rows.len()).sum();
        assert_eq!(rows, all.len(), "a row without a command");
        let strays: Vec<String> = pages
            .iter()
            .filter(|p| p.title.starts_with("More commands"))
            .flat_map(|p| p.rows.iter().map(|r| r.command.clone()))
            .collect();
        assert!(strays.is_empty(), "no area claims {strays:?}: add them to AREAS");
    }

    #[test]
    fn every_top_level_command_has_a_row() {
        let tree = tree();
        let pages = build(&tree, Lang::En);
        for command in &tree {
            let slash = format!("/{}", command.name);
            let under = format!("{slash} ");
            assert!(
                pages
                    .iter()
                    .flat_map(|p| &p.rows)
                    .any(|r| r.command == slash || r.command.starts_with(&under)),
                "/{} has no row",
                command.name
            );
        }
    }

    #[test]
    fn the_areas_are_well_formed() {
        let all = leaves(&tree(), Lang::En);
        for (i, area) in AREAS.iter().enumerate() {
            assert!(!area.covers.is_empty(), "{} covers nothing", area.key);
            for cover in area.covers {
                assert!(
                    all.iter().any(|l| crate::routing::legacy(&l.path) == *cover
                        || crate::routing::legacy(&l.path).starts_with(&format!("{cover} "))),
                    "`{cover}` in {} is not a command",
                    area.key
                );
                for other in &AREAS[i + 1..] {
                    assert!(
                        !other.covers.contains(cover),
                        "`{cover}` is in {} and in {}",
                        area.key,
                        other.key
                    );
                }
            }
        }
        for leaf in &all {
            assert!(area_of(&leaf.path).is_some(), "/{} has no area", leaf.path);
        }
        for (path, _) in WORDS {
            assert!(all.iter().any(|l| l.path == *path), "`{path}` is not a command");
        }
    }

    #[test]
    fn the_longest_start_of_a_path_wins() {
        let area = |path| area_of(path).map(|i| AREAS[i].title(Lang::En));
        let some = |title: &str| Some(title.to_owned());
        assert_eq!(area("play stats"), some("Profile & cards"));
        assert_eq!(area("play wordle"), some("Puzzles & word games"));
        assert_eq!(area("play derby"), some("Crowd games & collecting"));
        assert_eq!(area("config xp multiplier"), some("Levels, XP & points setup"));
        assert_eq!(area("config show"), some("Server setup"));
        assert_eq!(area("house contest vote"), some("House & plaza"));
        assert_eq!(area("plays"), None, "a longer word is not the same command");
        assert_eq!(area("config"), None, "no area holds a bare `config`");
    }

    #[test]
    fn every_row_fits_its_card_and_every_page_fits_a_card() {
        for lang in Lang::ALL {
            let pages = build(&tree(), lang);
            for page in &pages {
                let mut height = 0.0;
                for row in &page.rows {
                    assert!(
                        HelpCard::fits(row),
                        "{} would be cut on its card: {:?}",
                        row.command,
                        row.text
                    );
                    height += HelpCard::row_height(row);
                }
                assert!(height <= HelpCard::BODY_MAX, "{} is {height} px tall", page.title);
                assert!(!page.rows.is_empty(), "{} is empty", page.title);
            }
        }
    }

    #[test]
    fn the_menu_offers_every_page_within_discords_limits() {
        for lang in Lang::ALL {
            let pages = build(&tree(), lang);
            for at in 0..pages.len() {
                let rows = controls(&pages, at, lang);
                assert_eq!(rows.len(), 2);
                let Component::ActionRow(menu) = &rows[0] else {
                    panic!("the menu comes first")
                };
                let Component::SelectMenu(menu) = &menu.components[0] else {
                    panic!("a select menu")
                };
                let options = menu.options.as_deref().unwrap_or_default();
                assert_eq!(options.len(), pages.len().min(25));
                assert_eq!(options.iter().filter(|o| o.default).count(), 1);
                assert!(options.iter().any(|o| o.default && o.value == at.to_string()));
                for option in options {
                    assert!(option.label.chars().count() <= 100 && option.value.len() <= 100);
                    assert!(option.description.as_ref().is_some_and(|d| d.chars().count() <= 100));
                }
                let Component::ActionRow(buttons) = &rows[1] else {
                    panic!("the buttons come second")
                };
                let disabled: Vec<bool> = buttons
                    .components
                    .iter()
                    .map(|c| matches!(c, Component::Button(b) if b.disabled))
                    .collect();
                assert_eq!(disabled, [at == 0, at + 1 == pages.len()], "page {at}");
            }
        }
    }

    #[test]
    fn managers_are_marked_where_the_command_asks_for_them() {
        let pages = build(&tree(), Lang::Tr);
        let manager = |path: &str| rows_of(&pages, path)[0].manager;
        assert!(manager("setup server show") && manager("giveaway start") && manager("community server plaza-edit"));
        assert!(!manager("house view") && !manager("duel chess"));
    }
}

#[cfg(test)]
mod transition_tests {
    use super::*;
    #[test]
    fn existing_help_buttons_keep_their_action_destinations() {
        for lang in Lang::ALL {
            let tree = crate::command_definitions();
            let pages = build(&tree, lang);
            let mut previous = crate::commands::legacy_definitions();
            crate::routing::extend(&mut previous);
            for (at, page) in build_legacy(&previous, lang).iter().enumerate() {
                let destination = &pages[legacy_page(at, lang, &pages)];
                let old = page.rows[0].command.trim_start_matches('/');
                assert!(
                    destination
                        .rows
                        .iter()
                        .any(|r| r.command == format!("/{}", crate::routing::canonical(old)))
                );
            }
        }
    }
}
