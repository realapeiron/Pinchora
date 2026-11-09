use super::*;
use quote::ToTokens;
use syn::spanned::Spanned;

mod keyword {
    syn::custom_keyword!(fallback);
}

impl Parse for Registration {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut registration = Self::default();
        while !input.is_empty() {
            let section = input.parse::<Ident>()?.to_string();
            let body;
            braced!(body in input);
            let mut entries = BTreeMap::new();
            let mut sources = BTreeMap::new();
            while !body.is_empty() {
                let index = entries.len();
                if body.peek(keyword::fallback) {
                    body.parse::<keyword::fallback>()?;
                    registration.fallbacks.entry(section.clone()).or_default().insert(index);
                }
                let (tokens, source) = entry(&section, &body)?;
                entries.insert(index, tokens);
                if let Some(source) = source {
                    sources.insert(index, source);
                }
                if body.peek(syn::Token![,]) {
                    body.parse::<syn::Token![,]>()?;
                }
            }
            registration.sources.insert(section.clone(), sources);
            if registration.sections.insert(section, entries).is_some() {
                return Err(input.error("duplicate registration section"));
            }
        }
        Ok(registration)
    }
}

fn entry(section: &str, input: ParseStream<'_>) -> syn::Result<(TokenStream, Option<String>)> {
    if matches!(section, "owner_prelude") {
        let expression: syn::Expr = input.parse()?;
        return Ok((expression.to_token_stream(), expression.span().source_text()));
    }
    if matches!(section, "command_gate_overrides" | "component_gate_overrides") {
        let expression: syn::Expr = input.parse()?;
        return Ok((expression.to_token_stream(), expression.span().source_text()));
    }
    if matches!(
        section,
        "components"
            | "modals"
            | "owners"
            | "owner_actions"
            | "command_gates"
            | "component_gates"
            | "autocomplete"
            | "defer_overrides"
    ) {
        let arm: syn::Arm = input.parse()?;
        return Ok((arm.to_token_stream(), arm.span().source_text()));
    }
    let expression: syn::Expr = input.parse()?;
    let mut tokens = expression.to_token_stream();
    let mut source = expression.span().source_text();
    // Slice entries need a separator; route tuples and legacy leaves are parsed individually.
    if !matches!(section, "routes" | "legacy_leaves") {
        tokens.extend(quote::quote!(,));
        source = source.map(|s| format!("{s},"));
    }
    Ok((tokens, source))
}

pub(super) fn data(registration: &mut Registration, path: &Path) -> Result {
    if !path.exists() {
        return Ok(());
    }
    let values: BTreeMap<String, Vec<serde_json::Value>> = serde_json::from_str(&fs::read_to_string(path)?)?;
    for (section, values) in values {
        if registration.sections.contains_key(&section) {
            return Err(format!("{section} declared in both Rust and data").into());
        }
        let entries = values
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                let raw = match value {
                    serde_json::Value::Array(values) => format!(
                        "({})",
                        values.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
                    ),
                    serde_json::Value::String(value) => serde_json::to_string(&value).unwrap(),
                    _ => return Err("registration data must be strings or tuples".into()),
                };
                let raw = if matches!(section.as_str(), "routes" | "legacy_leaves") {
                    raw
                } else {
                    format!("{raw},")
                };
                Ok((index, raw.parse()?))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        registration.sections.insert(section, entries);
    }
    Ok(())
}

pub(super) fn commands(registration: &mut Registration, mac: &syn::Macro) -> Result {
    let parsed: Commands = mac.parse_body()?;
    let entries = registration.sections.entry("commands".into()).or_default();
    for key in parsed.0 {
        entries.insert(entries.len(), quote::quote!(#key));
    }
    Ok(())
}

struct Commands(Vec<String>);
impl Parse for Commands {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut keys = Vec::new();
        while !input.is_empty() {
            let kind: Ident = input.parse()?;
            let body;
            braced!(body in input);
            while !body.is_empty() {
                let key = if kind == "typed" {
                    source::tokens(&body.parse::<syn::Type>()?)
                } else if kind == "context" {
                    let tuple: syn::ExprTuple = body.parse()?;
                    source::strings(&tuple)[0].clone()
                } else {
                    return Err(input.error("expected typed or context commands"));
                };
                keys.push(key.chars().filter(|c| !c.is_whitespace()).collect());
                body.parse::<syn::Token![,]>()?;
            }
        }
        Ok(Self(keys))
    }
}
