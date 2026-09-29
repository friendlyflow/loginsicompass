//! The greeter's accessibility settings: the object the login screen and
//! every desicompass session share, over the system defaults, over the
//! greeter's own built-in defaults.
//!
//! The shared object is `/var/lib/sicompass/accessibility.json`
//! (`sicompass_ui::accessibility::SHARED_PATH`), a flat object with the app's
//! own `settings.json` keys (`screenReader`, `fontScale`, …). sicompass and
//! desicompass-superkey read and write it too, so what is chosen here is what
//! the session starts with, and what is chosen in a session is what this
//! screen shows next time. The layer below is
//! `/etc/sicompass/accessibility.json`, which the desicompass NixOS module
//! writes. Both are read through [`SharedAccessibility`]: writes are atomic
//! and locked, and a change to either while the greeter runs is picked up by
//! [`Settings::poll`].
//!
//! The one built-in default that differs from the app's is `screenReader`: on
//! here. The first time the greeter runs, nobody has chosen anything yet, and a
//! blind user cannot turn on a screen reader they cannot hear. Once someone
//! unticks it, that is saved and the greeter stays quiet.

use std::path::Path;

use sicompass_ui::accessibility::{AccessibilitySettings, SharedAccessibility, font_scale_value};

/// The shared object's file name, in the directory [`Settings::load`] is given.
pub const FILE_NAME: &str = "accessibility.json";

/// Where the greeter kept its choices, in its state directory, before they
/// were shared. Read once by [`Settings::migrate_from`].
pub const OLD_FILE_NAME: &str = "settings.json";

/// The resolved settings, and where to save changes.
pub struct Settings {
    shared: SharedAccessibility,
    /// Choices that could not be saved (a read-only state directory). They
    /// still apply for this run.
    unsaved: AccessibilitySettings,
}

fn builtin() -> AccessibilitySettings {
    AccessibilitySettings {
        screen_reader: Some(true),
        ..AccessibilitySettings::builtin()
    }
}

impl Settings {
    /// Load the shared choices from [`FILE_NAME`] in `dir` (normally
    /// `/var/lib/sicompass`) and the system defaults from `defaults_file`.
    /// Neither has to exist.
    pub fn load(dir: &Path, defaults_file: &Path) -> Self {
        Self {
            shared: SharedAccessibility::new(
                Some(dir.join(FILE_NAME)),
                vec![defaults_file.to_path_buf()],
                builtin(),
            ),
            unsaved: AccessibilitySettings::default(),
        }
    }

    fn resolved(&self) -> AccessibilitySettings {
        self.unsaved.clone().or(self.shared.effective())
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
    /// unknown key or an invalid value. A failure to save is logged and
    /// otherwise ignored: the choice still applies for this run.
    pub fn set(&mut self, key: &str, value: &str) -> bool {
        match self.shared.set(key, value) {
            Ok(()) => true,
            Err(e) if e.kind() == std::io::ErrorKind::InvalidInput => false,
            Err(e) => {
                tracing::warn!("could not save the greeter settings: {e}");
                self.unsaved.set(key, value)
            }
        }
    }

    /// Settings that changed on disk since the last look, by someone else,
    /// with their new values. Throttled, so it can be asked every frame.
    pub fn poll(&mut self) -> Vec<(&'static str, String)> {
        self.shared.poll()
    }

    /// Bring over the choices the greeter saved in its own state file
    /// before they were shared, once: only while the shared object has none.
    /// Otherwise a screen reader someone switched off at the login screen
    /// would start talking again after the update.
    pub fn migrate_from(&mut self, old: &Path) {
        if self.shared.saved() != &AccessibilitySettings::default() || !old.exists() {
            return;
        }
        let before = AccessibilitySettings::load(old);
        for key in sicompass_ui::accessibility::ALL_KEYS {
            if let Some(value) = before.get(key) {
                self.set(key, &value);
            }
        }
        tracing::info!(
            "moved the greeter's settings from {} to the shared object",
            old.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sicompass_ui::accessibility::{KEY_FONT_SCALE, KEY_LANGUAGE, KEY_SCREEN_READER};
    use std::path::PathBuf;

    /// The system defaults, beside (never in place of) the shared object's
    /// own file in the same directory.
    fn defaults(dir: &Path, json: &str) -> PathBuf {
        let p = dir.join("etc.json");
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

    #[test]
    fn a_change_on_disk_is_picked_up() {
        let state = tempfile::tempdir().unwrap();
        let etc = defaults(state.path(), r#"{"colorScheme": "dark"}"#);
        let mut s = Settings::load(state.path(), &etc);
        std::fs::write(&etc, r#"{"colorScheme": "light"}"#).unwrap();
        // Past the poll's throttle.
        std::thread::sleep(sicompass_ui::accessibility::POLL_INTERVAL);
        assert_eq!(s.poll(), vec![("colorScheme", "light".to_owned())]);
        assert_eq!(s.color_scheme(), "light");
        assert!(s.poll().is_empty());
    }

    #[test]
    fn a_choice_that_cannot_be_saved_still_applies() {
        let state = tempfile::tempdir().unwrap();
        // A file where the state directory should be: nothing can be saved.
        let blocked = state.path().join("blocked");
        std::fs::write(&blocked, "").unwrap();
        let mut s = Settings::load(&blocked, &state.path().join("none.json"));
        assert!(s.set(KEY_LANGUAGE, "de-BE"));
        assert_eq!(s.language(), "de-BE");
    }

    #[test]
    fn the_old_state_file_is_moved_over_once() {
        let state = tempfile::tempdir().unwrap();
        let shared = tempfile::tempdir().unwrap();
        let old = state.path().join(OLD_FILE_NAME);
        std::fs::write(&old, r#"{"screenReader": false, "fontScale": "2.00"}"#).unwrap();
        let none = state.path().join("none.json");

        let mut s = Settings::load(shared.path(), &none);
        s.migrate_from(&old);
        assert!(
            !s.screen_reader(),
            "switched off at the login screen, still off"
        );
        assert_eq!(s.font_scale(), "2.00");

        // Once there are shared choices, the old file has no say.
        assert!(s.set(KEY_FONT_SCALE, "1.25"));
        let mut s = Settings::load(shared.path(), &none);
        s.migrate_from(&old);
        assert_eq!(s.font_scale(), "1.25");
    }

    #[test]
    fn nothing_to_move_changes_nothing() {
        let shared = tempfile::tempdir().unwrap();
        let mut s = Settings::load(shared.path(), &shared.path().join("none.json"));
        s.migrate_from(&shared.path().join("absent.json"));
        assert!(!shared.path().join(FILE_NAME).exists());
    }
}
