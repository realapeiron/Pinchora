use pinchora_core::{
    i18n::{self, Lang},
    t,
};
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
