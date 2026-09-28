// SPDX-License-Identifier: MPL-2.0

//! Localization: Fluent messages selected from the system language.

use std::sync::LazyLock;

use i18n_embed::{
    fluent::{fluent_language_loader, FluentLanguageLoader},
    unic_langid::LanguageIdentifier,
    DefaultLocalizer, LanguageLoader, Localizer,
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Localizations;

/// A loader with the fallback (English) loaded. Tests use their own instances.
#[must_use]
pub fn new_loader() -> FluentLanguageLoader {
    let loader: FluentLanguageLoader = fluent_language_loader!();
    loader
        .load_fallback_language(&Localizations)
        .expect("Error while loading fallback language");
    // No invisible FSI/PDI marks around placeholders in UI text. Only affects loaded bundles,
    // so this must come after loading.
    loader.set_use_isolating(false);
    loader
}

pub static LANGUAGE_LOADER: LazyLock<FluentLanguageLoader> = LazyLock::new(new_loader);

/// Selects the best available translation for the requested languages (fallback: English).
pub fn select(langs: &[LanguageIdentifier]) {
    select_on(&LANGUAGE_LOADER, langs);
}

fn select_on(loader: &FluentLanguageLoader, langs: &[LanguageIdentifier]) {
    let localizer = DefaultLocalizer::new(loader, &Localizations);
    if let Err(why) = localizer.select(langs) {
        eprintln!("error while loading fluent localizations: {why}");
    }
    // Loading languages rebuilds the bundles, so isolation must be turned off again.
    loader.set_use_isolating(false);
}

/// `pt_BR`, `pt_BR.UTF-8`, `en_US@euro` -> `pt-BR`, `pt-BR`, `en-US`; `C`/`POSIX`/empty -> None.
#[must_use]
pub fn parse_accounts_language(raw: &str) -> Option<LanguageIdentifier> {
    let base = raw.split(['.', '@']).next()?.trim();
    if base.is_empty() || base == "C" || base == "POSIX" {
        return None;
    }
    base.replace('_', "-").parse().ok()
}

#[must_use]
pub fn parse_languages<S: AsRef<str>>(raw: &[S]) -> Vec<LanguageIdentifier> {
    raw.iter().filter_map(|s| parse_accounts_language(s.as_ref())).collect()
}

/// Request a localized string by ID from the i18n/ directory.
#[macro_export]
macro_rules! fl {
    ($message_id:literal) => {{
        i18n_embed_fl::fl!($crate::i18n::LANGUAGE_LOADER, $message_id)
    }};

    ($message_id:literal, $($args:expr),*) => {{
        i18n_embed_fl::fl!($crate::i18n::LANGUAGE_LOADER, $message_id, $($args), *)
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn loader_for(tag: &str) -> FluentLanguageLoader {
        let l = new_loader();
        let id: LanguageIdentifier = tag.parse().unwrap();
        select_on(&l, &[id]);
        l
    }

    #[test]
    fn parses_accounts_service_values() {
        assert_eq!(parse_accounts_language("pt_BR"), "pt-BR".parse().ok());
        assert_eq!(parse_accounts_language("pt_BR.UTF-8"), "pt-BR".parse().ok());
        assert_eq!(parse_accounts_language("en_US@euro"), "en-US".parse().ok());
        assert_eq!(parse_accounts_language("en"), "en".parse().ok());
        assert_eq!(parse_accounts_language("C"), None);
        assert_eq!(parse_accounts_language("POSIX"), None);
        assert_eq!(parse_accounts_language(""), None);
        assert_eq!(parse_accounts_language("  "), None);
    }

    #[test]
    fn parse_languages_skips_unusable_entries() {
        let got = parse_languages(&["pt_BR", "C", "en_US.UTF-8"]);
        let want: Vec<LanguageIdentifier> = vec!["pt-BR".parse().unwrap(), "en-US".parse().unwrap()];
        assert_eq!(got, want);
    }

    #[test]
    fn portuguese_and_english_differ() {
        assert_eq!(loader_for("en").get("tab-main"), "Main");
        assert_eq!(loader_for("pt-BR").get("tab-main"), "Principal");
    }

    #[test]
    fn unknown_language_falls_back_to_english() {
        assert_eq!(loader_for("fr").get("tab-main"), "Main");
    }

    #[test]
    fn fresh_loader_has_no_isolation_marks() {
        let l = new_loader();
        let s = l.get_args("status-on", std::collections::HashMap::<&str, &str>::from([("ssid", "MyNetwork")]));
        assert_eq!(s, "On — SSID MyNetwork");
    }

    #[test]
    fn placeholders_have_no_isolation_marks() {
        let l = loader_for("en");
        let s = l.get_args("status-on", std::collections::HashMap::<&str, &str>::from([("ssid", "MyNetwork")]));
        assert_eq!(s, "On — SSID MyNetwork");
    }

    fn ids(src: &str) -> BTreeSet<String> {
        src.lines()
            .filter_map(|l| {
                let (k, _) = l.split_once('=')?;
                let k = k.trim();
                let ok = !l.starts_with(' ')
                    && !l.starts_with('#')
                    && !k.is_empty()
                    && k.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
                ok.then(|| k.to_string())
            })
            .collect()
    }

    #[test]
    fn english_and_portuguese_define_the_same_messages() {
        let en = ids(include_str!("../i18n/en/cosmic_applet_hotspot.ftl"));
        let pt = ids(include_str!("../i18n/pt-BR/cosmic_applet_hotspot.ftl"));
        assert!(en.len() >= 20, "parsed too few ids: {en:?}");
        assert_eq!(en, pt);
    }
}
