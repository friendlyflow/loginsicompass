//! The greeter, drawn by `sicompass-ui`.
//!
//! This is the `--render-backend gpu` role: an SDL3 + Vulkan window running the
//! app's own renderer with exactly one provider in it. Everything that makes
//! the login screen usable by a screen-reader user — the AccessKit tree, the
//! masked `<password>`, the spoken per-keystroke echo, the live region — comes
//! from that renderer and is not reimplemented here.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use sicompass_sdk::ffon::IdArray;
use sicompass_ui::accessibility::{
    self, KEY_FONT_SCALE, KEY_LANGUAGE, KEY_SCREEN_READER, ScreenReader,
};
use sicompass_ui::app_state::{AppConfig, AppRenderer, AppState, PaletteTheme};
use sicompass_ui::registry::HostHooks;

use crate::auth::GreetdWorker;
use crate::greetd::GreetdClient;
use crate::i18n::{t, t_with};
use crate::provider::LoginProvider;
use crate::settings::Settings;
use crate::{lastlogin, power, sessions, users};

/// The greeter's answers to the renderer.
///
/// Applying settings, the font scale they set, and when to quit. The rest of
/// the defaults are already right: there is no updater and no second tab.
struct GreeterHooks {
    done: Arc<AtomicBool>,
    font_scale: Mutex<f32>,
    /// Dropped with the renderer, which stops Orca before greetd hands the
    /// display to the session.
    screen_reader: Mutex<ScreenReader>,
    clock_focused: Arc<AtomicBool>,
    clock_row: usize,
}

impl HostHooks for GreeterHooks {
    /// Called every frame (the greeter always has a settings queue), which is
    /// also what makes it the place to tell the provider where the cursor is,
    /// and to keep its failure in the header.
    fn apply_pending_settings(&self, r: &mut AppRenderer, _initial: bool) {
        let id = r.current_id.as_slice();
        self.clock_focused
            .store(id == [0, self.clock_row], Ordering::Relaxed);

        // The renderer clears the header whenever it rebuilds the list, which
        // moving into or out of a group does. Put the failure back, after the
        // key handling and in the same frame, so it stays on screen and the
        // screen reader, which speaks a header error when it changes, does not
        // hear it disappear and come back as a new one.
        if let Some(err) = r.providers.get_mut(0).and_then(|p| p.take_error()) {
            r.error_message = err;
        }

        let Some(queue) = r.settings_queue.clone() else {
            return;
        };
        let pending = std::mem::take(&mut *queue.lock().unwrap_or_else(|e| e.into_inner()));
        for (key, value) in pending {
            self.apply(r, &key, &value);
        }
    }

    fn read_font_scale(&self) -> f32 {
        *self.font_scale.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn should_quit(&self) -> bool {
        self.done.load(Ordering::Relaxed)
    }
}

impl GreeterHooks {
    fn apply(&self, r: &mut AppRenderer, key: &str, value: &str) {
        if key == KEY_FONT_SCALE {
            // Stored before the rebuild is flagged: the rebuild reads it back
            // through `read_font_scale`.
            *self.font_scale.lock().unwrap_or_else(|e| e.into_inner()) =
                accessibility::font_scale_value(Some(value));
        }
        if accessibility::apply_display(r, key, value) {
            return;
        }
        match key {
            KEY_LANGUAGE => {
                sicompass_sdk::localize::set_locale(value);
                relocalize(r);
                // Said in the new language, so the screen reader switches voice
                // even though the focused row has not moved.
                r.speak_language_change();
            }
            KEY_SCREEN_READER => {
                let mut sr = self.screen_reader.lock().unwrap_or_else(|e| e.into_inner());
                // No announcement of our own either way: Orca says "screen
                // reader on" when it starts and "screen reader off" when it is
                // asked to quit, in a voice that is certainly there.
                if value == "true" {
                    match sr.start() {
                        // It attaches before it listens; the first cursor move
                        // tells it which row has focus.
                        Ok(()) => r.a11y_refocus_on_move = true,
                        Err(e) => {
                            let error = e.to_string();
                            tracing::warn!("could not start the screen reader: {error}");
                            r.announce_provider_line(t_with(
                                "login-screen-reader-failed",
                                &[("error", &error)],
                            ));
                        }
                    }
                } else {
                    sr.stop();
                }
            }
            _ => {}
        }
    }
}

/// Rebuild the page in the new language, keeping the cursor.
///
/// Refreshing only the level the cursor is on would leave the row above it in
/// the old language. Inside the Language group that is the group's own label,
/// and the renderer hands that label back to the provider on the next choice,
/// which would then not recognise it. The page carries each group's options,
/// so replacing the provider's whole tree covers every level at once.
fn relocalize(r: &mut AppRenderer) {
    sicompass_ui::provider::refresh_all_provider_root_keys(r);
    let Some(pi) = r.current_id.get(0) else {
        return;
    };
    let inside_group = r.current_id.depth() >= 3;
    let Some(p) = r.providers.get_mut(pi) else {
        return;
    };
    // `fetch` answers for the provider's current path; ask it for the page.
    if inside_group {
        p.pop_path();
    }
    let page = p.fetch();
    if inside_group {
        let label = r
            .current_id
            .get(1)
            .and_then(|i| page.get(i))
            .and_then(|e| e.as_obj())
            .map(|o| o.key.clone())
            .unwrap_or_default();
        p.push_path(&label);
    }
    if let Some(root) = r.ffon.get_mut(pi).and_then(|e| e.as_obj_mut()) {
        root.children = page;
    }
    sicompass_ui::list::create_list_current_layer(r);
}

/// What the greeter needs from the command line.
pub struct Options {
    pub state_dir: std::path::PathBuf,
    pub session_dirs: Vec<std::path::PathBuf>,
    pub extra_users: Vec<String>,
    pub power: power::Commands,
    /// The system accessibility defaults, normally
    /// `/etc/sicompass/accessibility.json`.
    pub defaults_file: std::path::PathBuf,
    /// The screen reader to start, normally Orca.
    pub screen_reader_command: std::path::PathBuf,
}

/// Build and run the login screen. Returns once greetd has taken the session
/// (or the user powered the machine off).
pub fn run(opts: &Options) -> Result<bool, String> {
    // A way to exercise the supervisor's fallback without breaking a driver.
    // Debug builds only: it must not be possible to talk a release greeter out
    // of starting. See `supervisor::decide`.
    if cfg!(debug_assertions) && std::env::var("SICOMPASS_FORCE_VULKAN_FAILURE").is_ok() {
        return Err("refusing to start: SICOMPASS_FORCE_VULKAN_FAILURE is set".to_owned());
    }

    let mut users = users::enumerate();
    for name in &opts.extra_users {
        if !users.iter().any(|u| &u.name == name) {
            users.push(users::UserEntry {
                name: name.clone(),
                uid: 0,
                full_name: None,
                shell: String::new(),
            });
        }
    }
    let sessions = sessions::enumerate(&opts.session_dirs);
    tracing::info!(
        "offering {} user(s) and {} session(s)",
        users.len(),
        sessions.len()
    );

    let last = lastlogin::Store::load(&opts.state_dir);

    // The language first, so nothing is ever shown or spoken in the wrong one.
    crate::i18n::init();
    let settings = Settings::load(&opts.state_dir, &opts.defaults_file);
    sicompass_sdk::localize::set_locale(&settings.language());

    // Orca before the window: the renderer waits a moment for a screen reader
    // to register before showing it, and one that is already starting has the
    // best chance of being there. It still starts listening for events only
    // after that, so the first cursor move re-announces focus (see
    // `AppRenderer::a11y_refocus_on_move`, armed below).
    let mut screen_reader = ScreenReader::new(&opts.screen_reader_command);
    let mut screen_reader_started = false;
    if settings.screen_reader() {
        match screen_reader.start() {
            Ok(()) => screen_reader_started = true,
            Err(e) => tracing::warn!(
                "could not start the screen reader {}: {e}",
                opts.screen_reader_command.display()
            ),
        }
    }
    let font_scale = settings.font_scale_value();
    let light = settings.color_scheme() == "light";
    let privacy_blank = settings.shoulder_surfing_protection();

    // No greetd is not fatal: the UI still comes up, which is what makes it
    // possible to run the greeter nested during development.
    let worker = match GreetdClient::connect() {
        Ok(client) => Some(GreetdWorker::spawn(client)),
        Err(e) => {
            tracing::warn!("no greetd connection ({e}); authentication is disabled");
            None
        }
    };

    let provider = LoginProvider::new(users, sessions, last, opts.power.clone(), worker, settings);
    let password_row = provider.password_row();
    let done = Arc::clone(provider.done_flag());
    let queue = Arc::clone(provider.settings_queue());
    let hooks = GreeterHooks {
        done: Arc::clone(&done),
        font_scale: Mutex::new(font_scale),
        screen_reader: Mutex::new(screen_reader),
        clock_focused: Arc::clone(provider.clock_focused_flag()),
        clock_row: provider.clock_row(),
    };

    let cfg = AppConfig {
        title: t("login-provider-name"),
        app_name: "Loginsicompass".to_owned(),
        app_id: "loginsicompass".to_owned(),
        vulkan_app_name: "loginsicompass".to_owned(),
        // The compositor gives the greeter the whole output, but say so anyway:
        // this also works under a nested compositor for testing.
        fullscreen: true,
        // There is no pointer to click a titlebar with and nothing to minimise
        // to, so the app's own window controls would be dead pixels.
        custom_titlebar: false,
        maximized: false,
        window_icon: false,
        font_scale,
        ..AppConfig::default()
    };

    let mut app = AppState::with_providers(&cfg, vec![Box::new(provider)], Box::new(hooks))
        .map_err(|e| format!("could not start the graphical greeter: {e}"))?;
    app.renderer.settings_queue = Some(queue);
    app.renderer.palette_theme = if light {
        PaletteTheme::Light
    } else {
        PaletteTheme::Dark
    };
    app.renderer.privacy_blank = privacy_blank;
    app.renderer.a11y_refocus_on_move = screen_reader_started;

    // Land the cursor on the password field, the way the app lands a first-run
    // user on the onboarding line (`programs::focus_onboarding`).
    let mut id = IdArray::new();
    id.push(0);
    id.push(password_row);
    app.renderer.current_id = id;
    sicompass_ui::list::create_list_current_layer(&mut app.renderer);

    app.run();

    Ok(done.load(Ordering::Relaxed))
}
