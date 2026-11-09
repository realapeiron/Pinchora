use super::*;

type Pattern = Vec<Option<String>>;

fn alternatives(pattern: &syn::Pat) -> Result<Vec<Pattern>> {
    match pattern {
        syn::Pat::Lit(lit) => match &lit.lit {
            syn::Lit::Str(value) => Ok(vec![vec![Some(value.value())]]),
            _ => Err("registration patterns require string literals".into()),
        },
        syn::Pat::Wild(_) => Ok(vec![vec![None]]),
        syn::Pat::Ident(binding) => match &binding.subpat {
            Some((_, pattern)) => alternatives(pattern),
            None => Ok(vec![vec![None]]),
        },
        syn::Pat::Paren(paren) => alternatives(&paren.pat),
        syn::Pat::Or(or) => or
            .cases
            .iter()
            .map(alternatives)
            .collect::<Result<Vec<_>>>()
            .map(|v| v.concat()),
        syn::Pat::Tuple(tuple) => product(tuple.elems.iter()),
        syn::Pat::TupleStruct(tuple) => {
            let mut values = product(tuple.elems.iter())?;
            for value in &mut values {
                value.insert(0, Some(source::tokens(&tuple.path)));
            }
            Ok(values)
        }
        _ => Err(format!("unsupported registration pattern: {}", source::tokens(pattern)).into()),
    }
}

fn product<'a>(patterns: impl Iterator<Item = &'a syn::Pat>) -> Result<Vec<Pattern>> {
    let mut output = vec![vec![]];
    for pattern in patterns {
        let values = alternatives(pattern)?;
        output = output
            .into_iter()
            .flat_map(|prefix| values.iter().map(move |value| [prefix.clone(), value.clone()].concat()))
            .collect();
    }
    Ok(output)
}

fn overlaps(left: &Pattern, right: &Pattern) -> bool {
    left == &[None]
        || right == &[None]
        || (left.len() == right.len()
            && left
                .iter()
                .zip(right)
                .all(|(a, b)| a.is_none() || b.is_none() || a == b))
}

pub(super) fn check(registration: &Registration) -> Result {
    for section in [
        "components",
        "modals",
        "owners",
        "owner_actions",
        "command_gates",
        "component_gates",
        "autocomplete",
    ] {
        let mut seen: Vec<(usize, Vec<Pattern>, bool)> = Vec::new();
        for (index, tokens) in registration
            .sections
            .get(section)
            .into_iter()
            .flat_map(|entries| entries.iter())
        {
            let arm: syn::Arm = syn::parse2(tokens.clone())?;
            let patterns = alternatives(&arm.pat)?;
            let fallback = registration
                .fallbacks
                .get(section)
                .is_some_and(|indices| indices.contains(index));
            if patterns == vec![vec![None]] && !fallback {
                return Err(format!("{section} wildcard requires explicit fallback").into());
            }
            if fallback && (!matches!(arm.pat, syn::Pat::Wild(_)) || arm.guard.is_some()) {
                return Err(format!("{section} fallback must be an unconditional wildcard").into());
            }
            for (previous, earlier, was_fallback) in &seen {
                if patterns.iter().any(|p| earlier.iter().any(|q| overlaps(p, q))) && !(fallback ^ was_fallback) {
                    return Err(format!("unordered overlapping {section} arms {previous} and {index}; use one explicit fallback for a wildcard").into());
                }
            }
            seen.push((*index, patterns, fallback));
        }
    }
    Ok(())
}
