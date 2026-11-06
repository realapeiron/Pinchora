//! Every command as the public website lists it: the areas of `/help` in the same order, each with
//! its commands, what they take and which ones need Manage Server. Built from the registered command
//! tree, so a command added later shows up on the site by itself; `pinchora-web` writes the result to
//! `web/src/lib/commands.json` and a test fails while the committed file is out of date.

use pinchora_core::i18n::Lang;
use pinchora_game::Content;
use serde_json::{Value, json};
use twilight_model::application::command::{CommandOption, CommandOptionType};

use crate::commands::help::{AREAS, area_of, leaves};

/// Choices listed on the site for one option; longer lists are left to Discord's own menu.
const MAX_CHOICES: usize = 12;

/// `profile-and-cards` for "Profile & cards": the anchor an area has on the commands page.
fn slug(title: &str) -> String {
    let mut out = String::new();
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if c == '&' {
            out.push_str("-and-");
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// `/give <user> <amount>`: required options in angle brackets, optional ones in square brackets.
fn usage(path: &str, options: &[CommandOption]) -> String {
    let mut text = format!("/{path}");
    for option in options {
        let name = &option.name;
        if option.required.unwrap_or(false) {
            text.push_str(&format!(" <{name}>"));
        } else {
            text.push_str(&format!(" [{name}]"));
        }
    }
    text
}

fn option_json(option: &CommandOption) -> Value {
    let mut value = json!({
        "name": option.name,
        "description": option.description,
        "required": option.required.unwrap_or(false),
    });
    let choices: Vec<&str> = option
        .choices
        .iter()
        .flatten()
        .map(|choice| choice.name.as_str())
        .collect();
    if !choices.is_empty() && choices.len() <= MAX_CHOICES {
        value["choices"] = json!(choices);
    }
    value
}

/// The commands grouped in the areas of `/help`, as the website's JSON (the site is English):
/// `{ "count": n, "areas": [{ "key", "title", "intro", "commands": [{ "path", "usage", "description",
/// "manager", "options": [{ "name", "description", "required", "choices"? }] }] }] }`.
pub fn command_catalog() -> Value {
    let tree = crate::command_definitions();
    let leaves = leaves(&tree, Lang::En);
    let count = leaves.len();
    let mut by_area: Vec<Vec<(usize, Value)>> = AREAS.iter().map(|_| Vec::new()).collect();
    for leaf in leaves {
        let Some(area) = area_of(&leaf.path) else {
            continue;
        };
        // The order of the area's `covers` list, then the order commands were registered in.
        let order = AREAS[area]
            .covers
            .iter()
            .position(|c| leaf.path == *c || leaf.path.starts_with(&format!("{c} ")))
            .unwrap_or(0);
        let options: Vec<&CommandOption> = leaf
            .options
            .iter()
            .filter(|o| {
                !matches!(
                    o.kind,
                    CommandOptionType::SubCommand | CommandOptionType::SubCommandGroup
                )
            })
            .collect();
        by_area[area].push((
            order,
            json!({
                "path": leaf.path,
                "usage": usage(&leaf.path, &leaf.options),
                "description": leaf.text,
                "manager": leaf.manager,
                "options": options.into_iter().map(option_json).collect::<Vec<_>>(),
            }),
        ));
    }
    let areas: Vec<Value> = AREAS
        .iter()
        .zip(by_area)
        .map(|(area, mut commands)| {
            commands.sort_by_key(|(order, _)| *order);
            let title = area.title(Lang::En);
            json!({
                "key": slug(&title),
                "title": title,
                "intro": area.intro(Lang::En),
                "commands": commands.into_iter().map(|(_, c)| c).collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({ "count": count, "areas": areas })
}

/// How much there is to find, for the landing page's numbers: `{ commands, furniture, pets, fish, backgrounds,
/// badges }`, counted in the content the bot runs on, so the site never promises more or less than the bot has.
pub fn site_facts() -> Value {
    let content = Content::get();
    json!({
        "commands": leaves(&crate::command_definitions(), Lang::En).len(),
        "furniture": content.furniture.len(),
        "pets": content.pets.len(),
        "fish": content.fish.len(),
        "backgrounds": content.backgrounds.len(),
        "badges": content.badges.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_plain_words() {
        assert_eq!(slug("Profile & cards"), "profile-and-cards");
        assert_eq!(slug("Levels, XP & points setup"), "levels-xp-and-points-setup");
        assert_eq!(slug("Start here"), "start-here");
    }

    #[test]
    fn every_command_is_listed_once_and_every_area_has_commands() {
        let catalog = command_catalog();
        let tree = crate::command_definitions();
        let all = leaves(&tree, Lang::En);
        let listed: Vec<&str> = catalog["areas"]
            .as_array()
            .expect("areas")
            .iter()
            .flat_map(|a| a["commands"].as_array().expect("commands"))
            .map(|c| c["path"].as_str().expect("path"))
            .collect();
        assert_eq!(listed.len(), all.len(), "a command is missing or listed twice");
        assert_eq!(catalog["count"].as_u64(), Some(all.len() as u64));
        for leaf in &all {
            assert_eq!(
                listed.iter().filter(|p| **p == leaf.path).count(),
                1,
                "/{} must be listed once",
                leaf.path
            );
        }
        for area in catalog["areas"].as_array().expect("areas") {
            assert!(
                !area["commands"].as_array().expect("commands").is_empty(),
                "{} lists nothing",
                area["title"]
            );
        }
        let keys: std::collections::BTreeSet<&str> = catalog["areas"]
            .as_array()
            .expect("areas")
            .iter()
            .map(|a| a["key"].as_str().expect("key"))
            .collect();
        assert_eq!(keys.len(), AREAS.len(), "two areas share an anchor");
    }

    #[test]
    fn the_facts_count_what_the_bot_has() {
        let facts = site_facts();
        assert_eq!(facts["commands"], command_catalog()["count"]);
        for key in ["furniture", "pets", "fish", "backgrounds", "badges"] {
            assert!(facts[key].as_u64().is_some_and(|n| n > 0), "{key} counted nothing");
        }
    }

    #[test]
    fn usage_marks_required_and_optional_options() {
        let catalog = command_catalog();
        let find = |path: &str| {
            catalog["areas"]
                .as_array()
                .expect("areas")
                .iter()
                .flat_map(|a| a["commands"].as_array().expect("commands"))
                .find(|c| c["path"] == path)
                .unwrap_or_else(|| panic!("/{path} is not in the catalog"))
                .clone()
        };
        assert_eq!(find("give")["usage"], "/give <user> <amount>");
        assert_eq!(find("profile")["usage"], "/profile [user]");
        assert_eq!(find("config xp voice")["manager"], true);
        assert_eq!(find("daily")["manager"], false);
    }
}
