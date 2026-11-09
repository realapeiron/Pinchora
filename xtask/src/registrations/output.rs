use super::*;
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};
use syn::spanned::Spanned;

pub(super) fn replace(original: &str, registration: &Registration, consumed: &mut BTreeSet<String>) -> Result<String> {
    let marker = regex::Regex::new(r"// registrations:([a-z_]+):start\n[\s\S]*?// registrations:([a-z_]+):end")?;
    let mut result = original.to_owned();
    for capture in marker.captures_iter(original).collect::<Vec<_>>().into_iter().rev() {
        let section = &capture[1];
        if section != &capture[2] {
            return Err("mismatched registration markers".into());
        }
        if !registration.sections.contains_key(section)
            && !matches!(section, "root_budget" | "component_dispatch" | "component_owners")
        {
            return Err(format!("missing required registration section {section}").into());
        }
        consumed.insert(section.to_owned());
        if section == "component_dispatch" {
            consumed.insert("components".to_owned());
        }
        if section == "component_owners" {
            consumed.insert("owners".to_owned());
        }
        if section == "help_areas" {
            consumed.extend(
                source::strings(&registration.tokens(section))
                    .into_iter()
                    .map(|key| format!("help_{key}")),
            );
        }
        let content = format!(
            "// registrations:{section}:start\n{} // registrations:{section}:end",
            render(registration, section)?
        );
        let range = capture.get(0).unwrap();
        result.replace_range(range.range(), &content);
    }
    Ok(result)
}

pub(super) fn format(root: &Path, source: &str) -> Result<String> {
    let mut child = Command::new("rustfmt")
        .args(["--edition", "2024", "--config-path"])
        .arg(root.join("rustfmt.toml"))
        .args(["--config", "skip_children=true"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child.stdin.take().unwrap().write_all(source.as_bytes())?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(String::from_utf8(output.stderr)?.into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

pub(super) fn routes(registration: &Registration) -> Result<Vec<(String, String, u8)>> {
    let mut routes = registration
        .sections
        .get("routes")
        .ok_or("routes are required")?
        .values()
        .map(|tokens| {
            let tuple: syn::ExprTuple = syn::parse2(tokens.clone())?;
            let strings = source::strings(&tuple);
            if strings.len() != 2 || tuple.elems.len() != 3 {
                return Err("invalid route entry".into());
            }
            let slice = match &tuple.elems[2] {
                syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Int(n), ..
                }) => n.base10_parse()?,
                _ => return Err("route slice must be an integer".into()),
            };
            Ok((strings[0].clone(), strings[1].clone(), slice))
        })
        .collect::<Result<Vec<_>>>()?;
    routes.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(routes)
}

pub(super) fn data(registration: &Registration) -> Result<Vec<(PathBuf, String)>> {
    let routes = routes(registration)?;
    let entries = routes
        .iter()
        .map(|(legacy, canonical, slice)| {
            format!(
                "  {{\n    \"legacy\": {},\n    \"canonical\": {},\n    \"slice\": {slice}\n  }}",
                serde_json::to_string(legacy).unwrap(),
                serde_json::to_string(canonical).unwrap()
            )
        })
        .collect::<Vec<_>>();
    let mut leaves = source::strings(&registration.tokens("legacy_leaves"));
    leaves.sort();
    let leaves = leaves.join("\n") + "\n";
    Ok(vec![
        (
            "crates/pinchora-commands/src/infra/routing/manifest.json".into(),
            format!("[\n{}\n]\n", entries.join(",\n")),
        ),
        (
            "crates/pinchora-commands/src/infra/routing/legacy-leaves.txt".into(),
            leaves,
        ),
    ])
}

fn render(registration: &Registration, section: &str) -> Result<String> {
    if matches!(section, "command_gate_overrides" | "component_gate_overrides") {
        let mut chain = String::new();
        for tokens in registration.sections[section].values() {
            let tuple: syn::ExprTuple = syn::parse2(tokens.clone())?;
            if tuple.elems.len() != 2 {
                return Err("gate override needs a condition and result".into());
            }
            chain.push_str(&format!(
                "if {} {{ {} }} else ",
                tuple.elems[0]
                    .span()
                    .source_text()
                    .unwrap_or_else(|| source::tokens(&tuple.elems[0])),
                tuple.elems[1]
                    .span()
                    .source_text()
                    .unwrap_or_else(|| source::tokens(&tuple.elems[1]))
            ));
        }
        if section == "command_gate_overrides" {
            return Ok(format!(
                "let module = {chain} {{ Module::of_command(&data.name, subcommand(data)) }};"
            ));
        }
        return Ok(chain.trim_end().to_owned());
    }
    if section == "component_dispatch" {
        return Ok(include_str!("component.rs.txt").replace("/* dispatch arms */", &registration.raw("components")));
    }
    if section == "component_owners" {
        return Ok(format!(
            "pub(crate) fn owner(prefix: &str) -> Option<&'static str> {{ Some(match prefix {{ {} _ => return None, }}) }}",
            registration.raw("owners")
        ));
    }
    if section == "root_budget" {
        let routes = routes(registration)?;
        let roots: BTreeSet<_> = routes.iter().map(|r| r.1.split(' ').next().unwrap()).collect();
        return Ok(format!("pub const PROJECTED_ROOTS: usize = {};", roots.len()));
    }
    if section == "help_areas" {
        return Ok(source::strings(&registration.tokens(section))
            .into_iter()
            .map(|key| {
                format!(
                    "Area {{ key: {key:?}, covers: &[{}] }},",
                    registration.tokens(&format!("help_{key}"))
                )
            })
            .collect());
    }
    if section == "admin_roots" {
        return Ok(format!(
            "Some({})",
            source::strings(&registration.tokens(section))
                .iter()
                .map(|key| format!("{key:?}"))
                .collect::<Vec<_>>()
                .join(" | ")
        ));
    }
    if section == "defer_overrides" {
        return Ok(registration
            .raw(section)
            .lines()
            .map(|line| format!("            {line}"))
            .collect::<Vec<_>>()
            .join("\n"));
    }
    if !matches!(section, "command_gates" | "component_gates") {
        return Ok(registration.raw(section));
    }
    let mut arms: Vec<(String, String)> = vec![];
    for tokens in registration.sections[section].values() {
        let arm: syn::Arm = syn::parse2(tokens.clone())?;
        let pattern = source::tokens(&arm.pat);
        let body = source::tokens(&arm.body);
        if let Some(last) = arms.last_mut().filter(|last| last.1 == body) {
            last.0.push_str(&format!(" | {pattern}"));
        } else {
            arms.push((pattern, body));
        }
    }
    Ok(arms
        .into_iter()
        .map(|(pattern, body)| format!("{pattern} => {body},\n"))
        .collect())
}

pub(super) fn fixture(root: &Path) -> Result<(PathBuf, String)> {
    let folder = root.join("crates/pinchora-commands/src/commands/registration");
    let registration = read_registration(&folder.join("fixture.rs"))?;
    validation::components(root, &registration)?;
    let path = folder.join("fixture_output.rs");
    let original = fs::read_to_string(&path)?.replace("fixture-registrations:", "registrations:");
    let output = replace(&original, &registration, &mut BTreeSet::new())?;
    Ok((
        path,
        format(root, &output.replace("registrations:", "fixture-registrations:"))?,
    ))
}

pub(super) fn documentation(root: &Path, registration: &Registration) -> Result<(PathBuf, String)> {
    let path = PathBuf::from("docs/releases/command-tree.md");
    let original = fs::read_to_string(root.join(&path))?;
    let start = "<!-- registration-tree:start -->";
    let end = "<!-- registration-tree:end -->";
    let a = original.find(start).ok_or("command tree start marker missing")? + start.len();
    let b = original.find(end).ok_or("command tree end marker missing")?;
    let mut table = String::from("\n\n| Command | Legacy handler |\n| --- | --- |\n");
    let mut routes = routes(registration)?;
    routes.sort_by(|left, right| left.1.cmp(&right.1));
    for (legacy, canonical, _) in routes {
        table.push_str(&format!("| `{canonical}` | `{legacy}` |\n"));
    }
    Ok((path, format!("{}{}\n{}", &original[..a], table, &original[b..])))
}
