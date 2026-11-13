use pinchora_core::infra::i18n;
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use std::collections::BTreeMap;
pub(crate) fn strings(lang: Lang, area: &str) -> BTreeMap<String, String> {
    let prefix = format!("{area}.");
    i18n::keys(Lang::En)
        .into_iter()
        .filter_map(|key| {
            key.strip_prefix(&prefix)
                .map(|suffix| (suffix.to_owned(), t!(lang, key)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_control_strings_exclude_api_error_copy() {
        let strings = strings(Lang::En, "command_control");
        assert!(strings.keys().all(|key| !key.starts_with("web.errors.")));
        assert!(strings.contains_key("ui.title"));
    }
}
