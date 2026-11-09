//! The greeter's strings, in the four languages the app offers.
//!
//! The bundles go into the same process-global localizer the renderer uses
//! (`sicompass_sdk::localize`), so one `set_locale` switches the renderer's own
//! words and these together. Keys are prefixed `login-`: registering a key
//! another bundle already has is an error.

use std::sync::Once;

use sicompass_sdk::localize;

const BUNDLES: [(&str, &str); 4] = [
    ("en-US", include_str!("../locales/en-US.ftl")),
    ("nl-BE", include_str!("../locales/nl-BE.ftl")),
    ("fr-BE", include_str!("../locales/fr-BE.ftl")),
    ("de-BE", include_str!("../locales/de-BE.ftl")),
];

/// The login screen's own beginner's manual (Tab): `login-manual-01` to
/// `login-manual-16` in every bundle. The app's manual speaks of programs, tabs
/// and the Store, which a login screen does not have. Raise `lines` with a new
/// line, and follow sicompass-ui's `docs/manual-guidelines.md`.
pub const MANUAL: sicompass_ui::ManualText = sicompass_ui::ManualText {
    prefix: "login-manual",
    lines: 16,
};

/// Register the bundles. Idempotent, so every entry point (and every test) can
/// call it without caring whether another already has.
pub fn init() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        for (locale, source) in BUNDLES {
            if let Err(e) = localize::register_bundle(locale, source) {
                tracing::warn!("could not register the {locale} strings: {e}");
            }
        }
    });
}

/// A message in the active language.
pub fn t(key: &str) -> String {
    init();
    localize::t(key)
}

/// A message with named arguments, each passed as a string. Strings rather than
/// numbers on purpose: Fluent formats numbers per locale, and a year must never
/// come out as "2,026".
pub fn t_with(key: &str, args: &[(&str, &str)]) -> String {
    init();
    let mut a = localize::Args::new();
    for (k, v) in args {
        a.set(*k, v.to_string());
    }
    localize::t_args(key, &a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn keys(source: &str) -> BTreeSet<String> {
        source
            .lines()
            .filter(|l| !l.starts_with('#') && !l.starts_with(' '))
            .filter_map(|l| l.split_once(" = ").map(|(k, _)| k.trim().to_owned()))
            .filter(|k| !k.is_empty())
            .collect()
    }

    /// The tutorial rule, applied here: every string exists in all four
    /// languages. A key missing from one falls back to English, which a
    /// screen-reader user hears as the voice switching mid-sentence.
    #[test]
    fn every_locale_has_every_key() {
        let english = keys(BUNDLES[0].1);
        assert!(english.len() > 30, "parsed too few keys: {english:?}");
        for (locale, source) in &BUNDLES[1..] {
            let other = keys(source);
            let missing: Vec<_> = english.difference(&other).collect();
            let extra: Vec<_> = other.difference(&english).collect();
            assert!(missing.is_empty(), "{locale} is missing {missing:?}");
            assert!(
                extra.is_empty(),
                "{locale} has keys English lacks: {extra:?}"
            );
        }
    }

    #[test]
    fn every_key_is_prefixed() {
        for (locale, source) in BUNDLES {
            for k in keys(source) {
                assert!(k.starts_with("login-"), "{locale}: {k} lacks the prefix");
            }
        }
    }

    /// The manual is the lines `MANUAL` names, in every language, written to the
    /// manual's rules (no missing or extra line, no em dash, no semicolon).
    #[test]
    fn the_manual_follows_the_rules_in_every_language() {
        for (locale, source) in BUNDLES {
            if let Err(faults) =
                sicompass_ui::help_text::check_manual(source, MANUAL.prefix, MANUAL.lines)
            {
                panic!("{locale}.ftl:\n{faults}");
            }
        }
    }

    /// It is the login screen's manual, not the app's: it never sends the reader
    /// to something this screen does not have.
    #[test]
    fn the_manual_speaks_only_of_the_login_screen() {
        let english: String = sicompass_ui::help_text::messages(BUNDLES[0].1)
            .into_iter()
            .filter(|(k, _)| k.starts_with(MANUAL.prefix))
            .map(|(_, v)| v)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(english.contains("login screen"), "{english}");
        for absent in ["Store", "tutorial", "program", "Ctrl+T", "terminal"] {
            assert!(!english.contains(absent), "the manual mentions {absent}");
        }
    }

    #[test]
    fn the_bundles_register_and_resolve() {
        assert_eq!(t("login-label-password"), "Password");
        assert_eq!(
            t_with("login-starting", &[("session", "Desicompass")]),
            "Starting Desicompass"
        );
    }
}
