use twilight_interactions::command::{CommandInputData, CommandModel};

use crate::infra::text::parse::*;

fn tree() -> Vec<Command> {
    crate::command_definitions()
        .into_iter()
        .filter(|c| c.kind == twilight_model::application::command::CommandType::ChatInput)
        .collect()
}

fn ok<'a>(tree: &'a [Command], text: &str) -> Parsed<'a> {
    parse(tree, "p!", text, Lang::En).unwrap_or_else(|refusal| panic!("{text:?}: {refusal:?}"))
}

fn usage_of(tree: &[Command], text: &str) -> String {
    match parse(tree, "p!", text, Lang::En) {
        Err(Refusal::Usage(text)) => text,
        other => panic!("{text:?} gave {other:?}"),
    }
}

/// The options of the innermost level: the subcommand's, or the command's.
fn leaf_options(data: &CommandData) -> &[CommandDataOption] {
    let mut options = data.options.as_slice();
    while let Some(CommandDataOption {
        value: CommandOptionValue::SubCommand(inner) | CommandOptionValue::SubCommandGroup(inner),
        ..
    }) = options.first()
    {
        options = inner;
    }
    options
}

#[test]
fn quotes_keep_words_together() {
    let words: Vec<String> = tokens(r#"say "two words" text:"a b"  “curly one” x"#)
        .into_iter()
        .map(|t| t.text)
        .collect();
    assert_eq!(words, ["say", "two words", "text:a b", "curly one", "x"]);
    assert!(tokens(r#""a:b""#)[0].quoted);
    assert_eq!(tokens(r#"open "never closed"#)[1].text, "never closed");
    assert!(tokens("   ").is_empty());
    assert_eq!(tokens(r#""""#)[0].text, "", "an empty quote is an empty word");
}

#[test]
fn mentions_and_ids_become_snowflakes() {
    assert_eq!(
        snowflake("<@123456789012345678>", &["@!", "@"]),
        Some(123456789012345678)
    );
    assert_eq!(
        snowflake("<@!123456789012345678>", &["@!", "@"]),
        Some(123456789012345678)
    );
    assert_eq!(snowflake("123456789012345678", &["@"]), Some(123456789012345678));
    assert_eq!(snowflake("<@&123456789012345678>", &["@&"]), Some(123456789012345678));
    assert_eq!(snowflake("<#123456789012345678>", &["#"]), Some(123456789012345678));
    assert_eq!(
        snowflake("<@&123456789012345678>", &["@!", "@"]),
        None,
        "a role is no user"
    );
    assert_eq!(snowflake("<#123456789012345678>", &["@"]), None);
    assert_eq!(snowflake("12", &["@"]), None, "too short for a snowflake");
    assert_eq!(snowflake("<@7>", &["@"]), Some(7), "a mention needs no length");
    assert_eq!(snowflake("@everyone", &["@"]), None);
}

#[test]
fn names_are_any_case_and_aliases_work() {
    let tree = tree();
    assert_eq!(ok(&tree, "PROFILE").path, "profile");
    assert_eq!(ok(&tree, "Fish CAST").path, "fish cast");
    assert_eq!(ok(&tree, "bal").path, "balance");
    assert_eq!(ok(&tree, "lb").path, "leaderboard");
    assert_eq!(ok(&tree, "h").path, "help");
    for (alias, name) in ALIASES {
        assert!(
            tree.iter().any(|c| c.name == *name),
            "{alias} → /{name} is not a command"
        );
        assert!(!tree.iter().any(|c| c.name == *alias), "{alias} hides /{alias}");
    }
    assert_eq!(
        parse(&tree, "p!", "notacommand", Lang::En).unwrap_err(),
        Refusal::Unknown
    );
    assert_eq!(parse(&tree, "p!", "", Lang::En).unwrap_err(), Refusal::Unknown);
}

#[test]
fn a_choice_is_picked_by_value_or_name() {
    let tree = tree();
    let parsed = ok(&tree, "fish cast REEF");
    let options = leaf_options(&parsed.data);
    assert_eq!(options.len(), 1);
    assert!(matches!(&options[0].value, CommandOptionValue::String(s) if s == "reef"));
    let kelp = ok(&tree, "fish cast kelp");
    assert!(matches!(&leaf_options(&kelp.data)[0].value, CommandOptionValue::String(s) if s == "kelp_forest"));
    let deep = ok(&tree, "fish cast \"Deep Sea\"");
    assert!(matches!(&leaf_options(&deep.data)[0].value, CommandOptionValue::String(s) if s == "deep_sea"));
    let wrong = usage_of(&tree, "fish cast moon");
    assert!(wrong.starts_with("`spot` is one of: "), "{wrong}");
    assert!(wrong.ends_with("Usage: `p!fish cast [spot]`"), "{wrong}");
}

#[test]
fn a_wrong_call_explains_itself() {
    let tree = tree();
    let missing = usage_of(&tree, "duel connect4");
    assert_eq!(missing, "`user` is missing.\nUsage: `p!duel connect4 <user> [bet]`");
    let sub = usage_of(&tree, "fish");
    assert!(
        sub.starts_with("`fish` needs one more word.\nUsage: `p!fish <cast|"),
        "{sub}"
    );
    assert!(usage_of(&tree, "fish swim").starts_with("`fish` has no `swim`."));
    let low = usage_of(&tree, "duel connect4 <@123456789012345678> 5");
    assert!(low.starts_with("`bet` goes from 10 to 500."), "{low}");
    let word = usage_of(&tree, "duel connect4 <@123456789012345678> lots");
    assert!(word.starts_with("`bet` is a whole number."), "{word}");
    assert!(usage_of(&tree, "duel connect4 sam").starts_with("`user` is a member"));
    assert!(usage_of(&tree, "profile <@123456789012345678> extra").contains("one word too many"));
    // A member's word can't close the code span it is quoted in, nor make the reply too long.
    let sneaky = usage_of(&tree, "fish x`[nitro](https://e.x)`");
    assert!(sneaky.starts_with("`fish` has no `x[nitro](https://e.x)`."), "{sneaky}");
    assert!(usage_of(&tree, &format!("fish {}", "a".repeat(5_000))).chars().count() < 200);
    let twice = usage_of(
        &tree,
        "duel connect4 user:<@123456789012345678> user:<@223456789012345678>",
    );
    assert!(twice.starts_with("`user` is given twice."), "{twice}");
}

#[test]
fn named_values_and_mentions_are_read() {
    let tree = tree();
    let parsed = ok(&tree, "duel connect4 bet:50 <@!123456789012345678>");
    assert_eq!(parsed.users, [Id::new(123456789012345678)]);
    let options = leaf_options(&parsed.data);
    assert!(
        options
            .iter()
            .any(|o| o.name == "bet" && o.value == CommandOptionValue::Integer(50))
    );
    assert!(
        options
            .iter()
            .any(|o| o.name == "user" && o.value == CommandOptionValue::User(Id::new(123456789012345678)))
    );
    // A colon that names no option is just part of the word.
    let parsed = ok(&tree, "suggest text:\"open at 10:30\"");
    assert!(matches!(&leaf_options(&parsed.data)[0].value, CommandOptionValue::String(s) if s == "open at 10:30"));
    // Free text given by name goes on without quotes too.
    let parsed = ok(&tree, "suggest text:let us party");
    assert!(matches!(&leaf_options(&parsed.data)[0].value, CommandOptionValue::String(s) if s == "let us party"));
}

#[test]
fn free_text_takes_the_words_after_it() {
    let tree = tree();
    let parsed = ok(&tree, "suggest let us have a pool party");
    match &leaf_options(&parsed.data)[0].value {
        CommandOptionValue::String(text) => assert_eq!(text, "let us have a pool party"),
        other => panic!("{other:?}"),
    }
}

/// A value that fits the option, as a member would type it.
fn sample(option: &CommandOption) -> String {
    if let Some(choice) = option.choices.as_ref().and_then(|c| c.first()) {
        return match &choice.value {
            CommandOptionChoiceValue::String(s) => s.clone(),
            CommandOptionChoiceValue::Integer(n) => n.to_string(),
            CommandOptionChoiceValue::Number(n) => n.to_string(),
        };
    }
    let low = bound_number(option.min_value.as_ref());
    match option.kind {
        CommandOptionType::String => {
            let length = option.min_length.map_or(4, usize::from).max(4);
            let length = option.max_length.map_or(length, |max| length.min(usize::from(max)));
            if length >= 3 {
                format!("\"{} y\"", "x".repeat(length - 2))
            } else {
                "x".repeat(length)
            }
        }
        CommandOptionType::Integer => (low.unwrap_or(1.0) as i64).to_string(),
        CommandOptionType::Number => low.unwrap_or(1.0).to_string(),
        CommandOptionType::Boolean => "yes".to_owned(),
        CommandOptionType::User => "<@123456789012345678>".to_owned(),
        CommandOptionType::Role => "<@&223456789012345678>".to_owned(),
        CommandOptionType::Channel => "<#323456789012345678>".to_owned(),
        other => panic!("no sample for {other:?}"),
    }
}

/// Every leaf the tree registers: its path and its options.
fn leaves(tree: &[Command]) -> Vec<(String, Vec<CommandOption>)> {
    fn walk(path: &str, options: &[CommandOption], out: &mut Vec<(String, Vec<CommandOption>)>) {
        if !options.iter().any(is_branch) {
            out.push((path.to_owned(), options.to_vec()));
            return;
        }
        for option in options {
            walk(
                &format!("{path} {}", option.name),
                option.options.as_deref().unwrap_or_default(),
                out,
            );
        }
    }
    let mut out = Vec::new();
    for command in tree {
        walk(&command.name, &command.options, &mut out);
    }
    out
}

/// Every registered command parses its own usage: every option positionally, only the required
/// ones, and every option by name; and what comes out is what the handler's model reads.
#[test]
fn every_command_parses_its_own_usage() {
    let tree = tree();
    let leaves = leaves(&tree);
    assert!(leaves.len() > 100, "found only {} commands", leaves.len());
    for (path, options) in leaves {
        if options
            .iter()
            .any(|o| matches!(o.kind, CommandOptionType::Attachment | CommandOptionType::Mentionable))
        {
            continue;
        }
        let all: Vec<String> = options.iter().map(sample).collect();
        let required: Vec<String> = options
            .iter()
            .filter(|o| o.required == Some(true))
            .map(sample)
            .collect();
        let named: Vec<String> = options
            .iter()
            .rev()
            .map(|o| format!("{}:{}", o.name, sample(o)))
            .collect();
        for (form, words, expected) in [
            ("positional", &all, options.len()),
            ("required", &required, required.len()),
            ("named", &named, options.len()),
        ] {
            let text = format!("{path} {}", words.join(" "));
            let parsed = ok(&tree, &text);
            assert_eq!(parsed.path, path, "{form}: {text}");
            let given = leaf_options(&parsed.data);
            assert_eq!(given.len(), expected, "{form}: {text} gave {given:?}");
        }
        let shown = usage("p!", &path, &options, Lang::En);
        assert!(shown.starts_with(&format!("Usage: `p!{path}")), "{shown}");
    }
}

#[test]
fn whole_numbers_stop_at_what_discord_allows() {
    let option: CommandOption = serde_json::from_value(serde_json::json!({
        "type": 4,
        "name": "n",
        "description": "a number",
    }))
    .expect("option JSON");
    let read = |text: &str| value(&option, text, Lang::En);
    assert_eq!(read("50"), Ok(CommandOptionValue::Integer(50)));
    assert_eq!(read("-1,000"), Ok(CommandOptionValue::Integer(-1000)));
    assert_eq!(read("9007199254740992"), Ok(CommandOptionValue::Integer(MAX_INTEGER)));
    for huge in ["9007199254740993", "9223372036854775807", "99999999999999999999999999"] {
        assert_eq!(read(huge), Ok(CommandOptionValue::Integer(MAX_INTEGER)), "{huge}");
    }
    for huge in [
        "-9007199254740993",
        "-9223372036854775808",
        "-99999999999999999999999999",
    ] {
        assert_eq!(read(huge), Ok(CommandOptionValue::Integer(-MAX_INTEGER)), "{huge}");
    }
    assert!(read("12x").is_err() && read("").is_err() && read("-").is_err());
    // The option's own bounds still apply to the clamped number.
    let bounded: CommandOption = serde_json::from_value(serde_json::json!({
        "type": 4,
        "name": "bet",
        "description": "a bet",
        "min_value": 1,
        "max_value": 1000,
    }))
    .expect("option JSON");
    assert_eq!(value(&bounded, "1000", Lang::En), Ok(CommandOptionValue::Integer(1000)));
    assert_eq!(
        value(&bounded, "99999999999999999999", Lang::En),
        Err("`bet` goes from 1 to 1000.".to_owned())
    );
}

/// A wrong call is explained in the language it is parsed in, and `evet`/`hayır` answer a switch.
#[test]
fn a_wrong_call_explains_itself_in_turkish() {
    let tree = tree();
    let tr = |text: &str| match parse(&tree, "p!", text, Lang::Tr) {
        Err(Refusal::Usage(text)) => text,
        other => panic!("{text:?} gave {other:?}"),
    };
    assert_eq!(
        tr("duel connect4"),
        "`user` eksik.\nKullanım: `p!duel connect4 <user> [bet]`"
    );
    assert!(tr("fish swim").starts_with("`fish` komutunda `swim` diye bir şey yok."));
    assert!(tr("duel connect4 <@123456789012345678> 5").starts_with("`bet` 10 ile 500 arasında olmalı."));
    assert!(tr("fish cast moon").starts_with("`spot` şunlardan biri olmalı: "));
    let option: CommandOption = serde_json::from_value(serde_json::json!({
        "type": 5,
        "name": "enabled",
        "description": "a switch",
    }))
    .expect("option JSON");
    assert_eq!(value(&option, "Evet", Lang::Tr), Ok(CommandOptionValue::Boolean(true)));
    assert_eq!(
        value(&option, "HAYIR", Lang::Tr),
        Ok(CommandOptionValue::Boolean(false))
    );
    assert_eq!(
        value(&option, "belki", Lang::Tr),
        Err("`enabled` evet ya da hayır olmalı.".to_owned())
    );
}

/// What the parser builds is what twilight's derive reads for the handler, mentions included.
#[test]
fn the_handlers_model_reads_the_parsed_data() {
    let tree = tree();
    let parsed = ok(&tree, "fish cast shore");
    let input = CommandInputData::from(parsed.data);
    assert!(crate::commands::games::fish::FishCommand::from_interaction(input).is_ok());
}
