//! The greeter's accessibility settings: what it saved, over the system
//! defaults, over its own built-in defaults.
//!
//! Saved to `<state_dir>/settings.json` as a flat object with the app's own
//! `settings.json` keys (`screenReader`, `fontScale`, …). The layer below is
//! `/etc/sicompass/accessibility.json` (see `sicompass_ui::accessibility`),
//! which the desicompass NixOS module writes and the app reads too.
//!
//! The one built-in default that differs from the app's is `screenReader`: on
//! here. The first time the greeter runs, nobody has chosen anything yet, and a
//! blind user cannot turn on a screen reader they cannot hear. Once someone
//! unticks it, that is saved and the greeter stays quiet.

use std::path::{Path, PathBuf};

use sicompass_ui::accessibility::{AccessibilitySettings, font_scale_value};

pub const FILE_NAME: &str = "settings.json";

/// The resolved settings, and where to save changes.
pub struct Settings {
    path: PathBuf,
    /// Only what was chosen here. Saved as-is, so a system default that later
    /// changes is not frozen into this file.
    saved: AccessibilitySettings,
    defaults: AccessibilitySettings,
}

fn builtin() -> AccessibilitySettings {
    AccessibilitySettings {
        screen_reader: Some(true),
        font_scale: Some(format!("{:.2}", sicompass_ui::registry::DEFAULT_FONT_SCALE)),
        color_scheme: Some("dark".to_owned()),
        language: Some("en-US".to_owned()),
        shoulder_surfing_protection: Some(false),
    }
}

impl Settings {
    /// Load the saved choices from `state_dir` and the system defaults from
    /// `defaults_file`. Neither has to exist.
    pub fn load(state_dir: &Path, defaults_file: &Path) -> Self {
        let path = state_dir.join(FILE_NAME);
        Self {
            saved: AccessibilitySettings::load(&path),
            defaults: AccessibilitySettings::load(defaults_file).or(builtin()),
            path,
        }
    }

    fn resolved(&self) -> AccessibilitySettings {
        self.saved.clone().or(self.defaults.clone())
    }

    pub fn screen_reader(&self) -> bool {
        self.resolved().screen_reader.unwrap_or(true)
    }

    /// The stored form, e.g. `"1.75"`.
    pub fn font_scale(&self) -> String {
        self.resolved().font_scale.unwrap_or_default()
    }

    pub fn font_scale_value(&self) -> f32 {
        font_scale_value(Some(&self.font_scale()))
    }

    pub fn color_scheme(&self) -> String {
        self.resolved().color_scheme.unwrap_or_default()
    }

    pub fn language(&self) -> String {
        self.resolved().language.unwrap_or_default()
    }

    pub fn shoulder_surfing_protection(&self) -> bool {
        self.resolved().shoulder_surfing_protection.unwrap_or(false)
    }

    /// Record a choice and save it. Returns false, changing nothing, for an
    /// unknown key or an invalid value.
    pub fn set(&mut self, key: &str, value: &str) -> bool {
        if !self.saved.set(key, value) {
            return false;
        }
        self.save();
        true
    }

    /// Write-then-rename, like `lastlogin`. A failure is logged and otherwise
    /// ignored: the choice still applies for this run.
    fn save(&self) {
        let Some(dir) = self.path.parent() else {
            return;
        };
        let body = match serde_json::to_string_pretty(&serde_json::Value::Object(
            self.saved.to_object(),
        )) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("could not encode the greeter settings: {e}");
                return;
            }
        };
        let tmp = dir.join(format!(".{FILE_NAME}.tmp"));
        let result = std::fs::create_dir_all(dir)
            .and_then(|()| std::fs::write(&tmp, body))
            .and_then(|()| std::fs::rename(&tmp, &self.path));
        if let Err(e) = result {
            tracing::warn!("could not save {}: {e}", self.path.display());
            let _ = std::fs::remove_file(&tmp);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sicompass_ui::accessibility::{KEY_FONT_SCALE, KEY_LANGUAGE, KEY_SCREEN_READER};

    fn defaults(dir: &Path, json: &str) -> PathBuf {
        let p = dir.join("accessibility.json");
        std::fs::write(&p, json).unwrap();
        p
    }

    #[test]
    fn on_first_use_the_screen_reader_is_on() {
        let state = tempfile::tempdir().unwrap();
        let s = Settings::load(state.path(), &state.path().join("none.json"));
        assert!(s.screen_reader(), "a first run must speak");
        assert_eq!(s.font_scale(), "1.75");
        assert_eq!(s.color_scheme(), "dark");
        assert_eq!(s.language(), "en-US");
        assert!(!s.shoulder_surfing_protection());
    }

    #[test]
    fn the_system_defaults_beat_the_built_in_ones() {
        let state = tempfile::tempdir().unwrap();
        let etc = defaults(
            state.path(),
            r#"{"screenReader": false, "fontScale": "2.25", "language": "nl-BE"}"#,
        );
        let s = Settings::load(state.path(), &etc);
        assert!(!s.screen_reader());
        assert_eq!(s.font_scale(), "2.25");
        assert_eq!(s.language(), "nl-BE");
        assert_eq!(
            s.color_scheme(),
            "dark",
            "unset keys keep the built-in default"
        );
    }

    #[test]
    fn a_saved_choice_beats_the_system_default_and_survives_a_reload() {
        let state = tempfile::tempdir().unwrap();
        let etc = defaults(
            state.path(),
            r#"{"screenReader": true, "language": "fr-BE"}"#,
        );

        let mut s = Settings::load(state.path(), &etc);
        assert!(s.set(KEY_SCREEN_READER, "false"));
        assert!(!s.screen_reader());

        let s = Settings::load(state.path(), &etc);
        assert!(!s.screen_reader(), "unticking must be remembered");
        assert_eq!(s.language(), "fr-BE", "an unchosen key still follows /etc");
    }

    #[test]
    fn only_chosen_keys_are_saved() {
        let state = tempfile::tempdir().unwrap();
        let mut s = Settings::load(state.path(), &state.path().join("none.json"));
        assert!(s.set(KEY_FONT_SCALE, "2.00"));
        let body = std::fs::read_to_string(state.path().join(FILE_NAME)).unwrap();
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v, serde_json::json!({"fontScale": "2.00"}));
    }

    #[test]
    fn an_invalid_value_changes_nothing() {
        let state = tempfile::tempdir().unwrap();
        let mut s = Settings::load(state.path(), &state.path().join("none.json"));
        assert!(!s.set(KEY_LANGUAGE, "xx-XX"));
        assert_eq!(s.language(), "en-US");
        assert!(!state.path().join(FILE_NAME).exists());
    }

    #[test]
    fn a_corrupt_file_reads_as_nothing_chosen() {
        let state = tempfile::tempdir().unwrap();
        std::fs::write(state.path().join(FILE_NAME), "{oops").unwrap();
        let s = Settings::load(state.path(), &state.path().join("none.json"));
        assert!(s.screen_reader());
    }
}
