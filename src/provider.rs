//! The login screen, as a provider.
//!
//! Everything the greeter shows is an ordinary sicompass page: `<radio>`
//! groups, a `<password>` field, `<button>` rows and `<checkbox>` rows. That is
//! the point of building the greeter on `sicompass-ui` rather than drawing it
//! by hand — the masking, the screen-reader labels, the insert-mode editing and
//! the announcement live region all already work, and none of it is
//! reimplemented here.
//!
//! # There is no login button
//!
//! Pressing Enter in Insert mode on the password field submits, which is how
//! every other `<input>` in the app commits. A separate button would be a
//! second way to do one thing.
//!
//! # Only the show-password box below the password field
//!
//! greetd's prompt ("Password:" from an ordinary PAM stack) only repeats the
//! field's own label, so it is announced but not put on the page. Failures and
//! notices go to the renderer's header error line through `take_error`, which
//! speaks each one once and keeps it on screen until the next attempt.
//!
//! The "show password" box under the field is queued for the host like a
//! setting (`KEY_SHOW_PASSWORD`), which sets the renderer's
//! `password_revealed`. The field stays a `<password>`: only the masking is
//! the renderer's to drop. It is never saved.
//!
//! # Where the password lives
//!
//! In a [`Zeroizing<String>`], taken (not copied) when it is handed to the
//! worker. `fetch()` always emits an *empty* `<password></password>`: the live
//! typed value belongs to the host's insert buffer, which is what blanks the
//! field after every attempt without a special case here.
//!
//! # Settings
//!
//! The rows under the clock are the app's accessibility settings (all but the
//! update check). A change is saved at once and queued for the host, which
//! applies it to the renderer (`gui::GreeterHooks`): this provider never
//! touches the renderer or the screen reader itself.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use sicompass_sdk::ffon::FfonElement;
use sicompass_sdk::provider::Provider;
use sicompass_sdk::tags;
use sicompass_ui::accessibility::{
    COLOR_SCHEMES, FONT_SCALES, KEY_COLOR_SCHEME, KEY_FONT_SCALE, KEY_LANGUAGE, KEY_SCREEN_READER,
    KEY_SHOULDER_SURFING, LANGUAGES,
};
use sicompass_ui::registry::SettingsQueue;
use zeroize::Zeroizing;

use crate::auth::{GreetdCmd, GreetdEvent, GreetdWorker};
use crate::i18n::{t, t_with};
use crate::lastlogin;
use crate::power;
use crate::sessions::SessionEntry;
use crate::settings::Settings;
use crate::users::UserEntry;

/// The queue key for the "show password" box. Not an accessibility setting:
/// it goes to the host, never into the saved settings.
pub const KEY_SHOW_PASSWORD: &str = "showPassword";

/// The page's `<radio>` groups.
///
/// Identified by this rather than by their labels, which are translated: a
/// language change made from inside the Language group must not leave the
/// provider holding a path in the old language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    User,
    Session,
    FontScale,
    ColorScheme,
    Language,
}

impl Group {
    const ALL: [Group; 5] = [
        Group::User,
        Group::Session,
        Group::FontScale,
        Group::ColorScheme,
        Group::Language,
    ];

    /// The label, in the active language. It is also what `on_radio_change` is
    /// handed back, so it is matched on rather than re-derived.
    pub fn label(self) -> String {
        t(match self {
            Group::User => "login-group-user",
            Group::Session => "login-group-session",
            Group::FontScale => "login-setting-font-scale",
            Group::ColorScheme => "login-setting-color-scheme",
            Group::Language => "login-setting-language",
        })
    }

    fn from_label(label: &str) -> Option<Group> {
        Self::ALL.into_iter().find(|g| g.label() == label)
    }
}

/// Where the conversation with greetd has got to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// No attempt in flight and nothing asked yet.
    Idle,
    /// A request is out; greetd has not answered.
    Waiting,
    /// greetd asked something and is waiting for us.
    Prompting { secret: bool },
    /// The user is through; the session is being started.
    Starting,
    /// greetd accepted `start_session`. The greeter should exit so greetd can
    /// hand over the display.
    Done,
}

pub struct LoginProvider {
    /// `[]` at the page, one entry inside a group. `None` for a segment that
    /// named no group, so `pop_path` still pairs with `push_path`.
    path: Vec<Option<Group>>,

    users: Vec<UserEntry>,
    sessions: Vec<SessionEntry>,
    selected_user: usize,
    selected_session: usize,

    password: Zeroizing<String>,
    /// The "show password" box. Starts unticked every time the greeter does.
    show_password: bool,

    phase: Phase,
    /// The last notice or failure. Shown in the header until the next attempt,
    /// so it can be read again.
    message: Option<String>,
    /// The last prompt greetd sent for this user, so the same one asked again
    /// after a wrong password is not announced over "Wrong password".
    last_prompt: Option<String>,

    announcement: Option<String>,
    /// greetd is gone. Never cleared: nothing on this page can bring it back.
    fatal: Option<String>,

    greetd: Option<GreetdWorker>,
    last: lastlogin::Store,
    power: power::Commands,

    settings: Settings,
    /// Changes for the host to apply, as `(key, stored value)`.
    queue: SettingsQueue,

    /// The time the clock row shows, in Unix seconds. Kept as a time rather
    /// than a string so a language change re-renders it in the new language.
    clock: u64,
    /// Set by the host while the cursor is on the clock row. See
    /// [`refresh_clock`](Self::refresh_clock).
    clock_focused: Arc<AtomicBool>,
    /// Set when only the clock changed: drives `needs_refresh`, never `tick`.
    cosmetic: bool,
    /// Set when a greetd event changed something: drives `tick`.
    dirty: bool,

    /// Flipped once greetd has accepted `start_session`. The main loop reads it
    /// through `HostHooks::should_quit`, because greetd only launches the
    /// session after this process exits.
    done: Arc<AtomicBool>,
}

impl LoginProvider {
    pub fn new(
        users: Vec<UserEntry>,
        sessions: Vec<SessionEntry>,
        last: lastlogin::Store,
        power: power::Commands,
        greetd: Option<GreetdWorker>,
        settings: Settings,
    ) -> Self {
        crate::i18n::init();
        let user_names: Vec<String> = users.iter().map(|u| u.name.clone()).collect();
        let session_ids: Vec<String> = sessions.iter().map(|s| s.id.clone()).collect();
        let selected_user = lastlogin::index_of(&user_names, last.user());
        let selected_session = lastlogin::index_of(&session_ids, last.session());

        let mut me = Self {
            path: Vec::new(),
            users,
            sessions,
            selected_user,
            selected_session,
            password: Zeroizing::new(String::new()),
            show_password: false,
            phase: Phase::Idle,
            message: None,
            last_prompt: None,
            announcement: None,
            fatal: None,
            greetd,
            last,
            power,
            settings,
            queue: Arc::new(std::sync::Mutex::new(Vec::new())),
            clock: 0,
            clock_focused: Arc::new(AtomicBool::new(false)),
            cosmetic: false,
            dirty: false,
            done: Arc::new(AtomicBool::new(false)),
        };
        me.refresh_clock();
        me.begin_for_selected_user();
        // Whatever that reported is on the first page already; nothing needs
        // rebuilding yet.
        me.dirty = false;
        me
    }

    /// True once greetd has started the session and the greeter should quit.
    pub fn is_done(&self) -> bool {
        self.phase == Phase::Done
    }

    /// Shared with the render loop, which polls it once per frame.
    pub fn done_flag(&self) -> &Arc<AtomicBool> {
        &self.done
    }

    /// Shared with the host, which drains and applies it.
    pub fn settings_queue(&self) -> &SettingsQueue {
        &self.queue
    }

    /// Shared with the host, which sets it while the cursor is on the clock.
    pub fn clock_focused_flag(&self) -> &Arc<AtomicBool> {
        &self.clock_focused
    }

    fn current_user(&self) -> Option<&UserEntry> {
        self.users.get(self.selected_user)
    }

    fn current_session(&self) -> Option<&SessionEntry> {
        self.sessions.get(self.selected_session)
    }

    /// Put a notice or failure in the header. `dirty` makes the next tick
    /// rebuild, which is when the host reads `take_error`.
    fn report(&mut self, text: String) {
        self.message = Some(text);
        self.dirty = true;
    }

    /// Start (or restart) authentication for whoever is selected.
    fn begin_for_selected_user(&mut self) {
        let Some(user) = self.current_user().map(|u| u.name.clone()) else {
            self.report(t("login-no-accounts"));
            return;
        };
        let Some(greetd) = self.greetd.as_ref() else {
            // No socket: the UI still works, which is what makes it possible to
            // run the greeter nested for development.
            self.report(t("login-not-connected"));
            return;
        };
        greetd.send(GreetdCmd::Create { username: user });
        self.phase = Phase::Waiting;
        self.password = Zeroizing::new(String::new());
    }

    /// Hand the typed password to greetd.
    fn submit(&mut self) {
        // Take the secret *first*, before anything that can fail. Whatever
        // happens next, this provider's copy is gone and `Zeroizing` wipes the
        // buffer it came from: an early return must never leave a typed
        // password sitting in the field.
        let secret = std::mem::replace(&mut self.password, Zeroizing::new(String::new()));

        if !matches!(self.phase, Phase::Prompting { .. }) {
            self.fail_to_submit(t("login-not-ready"));
            return;
        }
        let Some(greetd) = self.greetd.as_ref() else {
            self.fail_to_submit(t("login-not-connected"));
            return;
        };
        greetd.send(GreetdCmd::Answer {
            response: Some(secret.to_string()),
        });
        self.phase = Phase::Waiting;
        self.message = None;
        self.dirty = true;
        self.announcement = Some(t("login-checking"));
    }

    /// Report a submit that never left the building.
    ///
    /// Spoken as well as shown: a screen-reader user who pressed Enter and
    /// heard nothing has no way to tell that from a slow PAM. When the header
    /// speaks it in the same frame, that replaces this rather than repeating it.
    fn fail_to_submit(&mut self, why: String) {
        self.announcement = Some(why.clone());
        self.report(why);
    }

    /// greetd accepted the credentials; ask it to launch the chosen session.
    fn start_session(&mut self) {
        let Some(session) = self.current_session().cloned() else {
            self.report(t("login-no-session"));
            self.phase = Phase::Idle;
            return;
        };
        let Some(greetd) = self.greetd.as_ref() else {
            return;
        };
        greetd.send(GreetdCmd::Start {
            cmd: session.exec.clone(),
            env: session.env(),
        });
        self.phase = Phase::Starting;
        self.announcement = Some(t_with("login-starting", &[("session", &session.name)]));
    }

    /// Drain everything the worker has produced. Returns true if the page changed.
    fn drain_greetd(&mut self) -> bool {
        let mut changed = false;
        loop {
            let Some(evt) = self.greetd.as_ref().and_then(|g| g.try_recv()) else {
                break;
            };
            changed = true;
            match evt {
                GreetdEvent::Prompt { secret, text } => {
                    self.phase = Phase::Prompting { secret };
                    // Spoken, not shown: see the module docs. But not the same
                    // question twice: after a wrong password greetd asks
                    // "Password:" again a moment later, and announcing it
                    // replaced "Wrong password" before it was heard.
                    if self.last_prompt.as_deref() != Some(text.as_str()) {
                        self.announcement = Some(text.clone());
                    }
                    self.last_prompt = Some(text);
                }
                GreetdEvent::Notice { text, .. } => {
                    self.message = Some(text);
                }
                GreetdEvent::Authenticated => self.start_session(),
                GreetdEvent::Started => {
                    // Only now is the choice worth remembering.
                    if let Some(u) = self.current_user() {
                        let name = u.name.clone();
                        self.last.set_user(&name);
                    }
                    if let Some(s) = self.current_session() {
                        let id = s.id.clone();
                        self.last.set_session(&id);
                    }
                    self.last.commit();
                    self.phase = Phase::Done;
                    self.done.store(true, Ordering::Relaxed);
                }
                GreetdEvent::Failed { auth, text } => {
                    if auth {
                        self.message = Some(t("login-wrong-password"));
                        // greetd keeps the conversation open after an auth
                        // error, but the simplest correct thing is to start the
                        // attempt over so we are never guessing which prompt is
                        // outstanding.
                        self.begin_for_selected_user();
                    } else {
                        self.message = Some(text);
                        self.phase = Phase::Idle;
                    }
                }
                GreetdEvent::Io { text } => {
                    self.fatal = Some(text);
                    self.phase = Phase::Idle;
                }
            }
        }
        changed
    }

    /// Move the clock on. Returns true if the row's text changed.
    ///
    /// Every second, except while the cursor is on the clock row: a focused
    /// row whose label changes is read out again, and a screen reader reading
    /// the time aloud every second would drown everything else. There it moves
    /// on the minute, which is as often as the old clock did.
    fn refresh_clock(&mut self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let stale = if self.clock_focused.load(Ordering::Relaxed) {
            now / 60 != self.clock / 60
        } else {
            now != self.clock
        };
        if !stale {
            return false;
        }
        self.clock = now;
        true
    }

    /// Record a settings change: save it, queue it for the host, and say what
    /// changed. Returns false for a value the settings refuse.
    fn change_setting(&mut self, key: &str, stored: &str) -> bool {
        if !self.settings.set(key, stored) {
            return false;
        }
        self.queue
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((key.to_owned(), stored.to_owned()));
        true
    }

    // ---- Page construction -------------------------------------------------

    /// A group's option labels and which one is selected.
    fn options(&self, group: Group) -> (Vec<String>, usize) {
        fn pick(all: &[&str], current: &str) -> usize {
            all.iter().position(|v| *v == current).unwrap_or(0)
        }
        match group {
            Group::User => (
                self.users.iter().map(|u| u.name.clone()).collect(),
                self.selected_user,
            ),
            Group::Session => (
                self.sessions.iter().map(|s| s.name.clone()).collect(),
                self.selected_session,
            ),
            Group::FontScale => (
                FONT_SCALES.iter().map(|s| s.to_string()).collect(),
                pick(FONT_SCALES, &self.settings.font_scale()),
            ),
            Group::ColorScheme => (
                COLOR_SCHEMES
                    .iter()
                    .map(|s| t(&format!("login-color-{s}")))
                    .collect(),
                pick(COLOR_SCHEMES, &self.settings.color_scheme()),
            ),
            Group::Language => (
                LANGUAGES
                    .iter()
                    .map(|s| t(&format!("login-language-{s}")))
                    .collect(),
                pick(LANGUAGES, &self.settings.language()),
            ),
        }
    }

    /// A group's options as rows, the selected one checked.
    fn option_rows(&self, group: Group) -> Vec<FfonElement> {
        let (labels, selected) = self.options(group);
        labels
            .into_iter()
            .enumerate()
            .map(|(i, l)| {
                FfonElement::Str(if i == selected {
                    tags::format_checked(&l)
                } else {
                    l
                })
            })
            .collect()
    }

    fn radio_group(&self, group: Group) -> FfonElement {
        let mut obj = FfonElement::new_obj(format!("<radio>{}", group.label()));
        let o = obj.as_obj_mut().expect("new_obj is an Obj");
        for row in self.option_rows(group) {
            o.push(row);
        }
        obj
    }

    fn checkbox(label: String, checked: bool) -> FfonElement {
        FfonElement::Str(if checked {
            tags::format_checkbox_checked(&label)
        } else {
            tags::format_checkbox(&label)
        })
    }

    /// The whole page, at path `/`.
    fn page(&self) -> Vec<FfonElement> {
        let mut out = Vec::new();

        if self.users.is_empty() {
            // Degradation path: nothing in /etc/passwd we can offer. Let the
            // user type a name rather than showing an empty group.
            out.push(FfonElement::Str(format!(
                "{}: {}",
                Group::User.label(),
                tags::format_input("")
            )));
        } else {
            out.push(self.radio_group(Group::User));
        }

        if !self.sessions.is_empty() {
            out.push(self.radio_group(Group::Session));
        }

        // Always empty: the live value lives in the host's insert buffer.
        out.push(FfonElement::Str(format!(
            "{}: {}",
            t("login-label-password"),
            tags::format_password("")
        )));
        out.push(Self::checkbox(t("login-show-password"), self.show_password));

        for (f, key) in [
            (power::SUSPEND, "login-button-suspend"),
            (power::REBOOT, "login-button-reboot"),
            (power::POWEROFF, "login-button-poweroff"),
        ] {
            out.push(FfonElement::Str(format!("<button>{f}</button>{}", t(key))));
        }

        out.push(FfonElement::Str(format_clock(self.clock)));

        // The settings, flat on the page rather than inside an object: one
        // arrow key away, not one level down.
        out.push(Self::checkbox(
            t("login-setting-screen-reader"),
            self.settings.screen_reader(),
        ));
        out.push(self.radio_group(Group::FontScale));
        out.push(self.radio_group(Group::ColorScheme));
        out.push(self.radio_group(Group::Language));
        out.push(Self::checkbox(
            t("login-setting-shoulder-surfing"),
            self.settings.shoulder_surfing_protection(),
        ));

        // Last, like the version lines on the app's settings page.
        out.push(FfonElement::Str(format!(
            "{}: {}",
            t("login-version"),
            env!("CARGO_PKG_VERSION")
        )));
        out
    }

    /// The index of the password row within [`page`], for landing the cursor.
    pub fn password_row(&self) -> usize {
        let mut i = 0;
        i += 1; // user group or typed-name row
        if !self.sessions.is_empty() {
            i += 1;
        }
        i
    }

    /// The index of the clock row within [`page`]: after the password field,
    /// the show-password box and the three power buttons.
    pub fn clock_row(&self) -> usize {
        self.password_row() + 5
    }
}

impl Provider for LoginProvider {
    fn name(&self) -> &str {
        "login"
    }

    fn display_name(&self) -> String {
        t("login-provider-name")
    }

    fn fetch(&mut self) -> Vec<FfonElement> {
        // Path-scoped: the whole page at the root, and just the options inside
        // a group. Returning the whole tree from inside a group would graft a
        // copy of the page under one of its own descendants — which is why
        // `refresh_current_directory` keeps a list of providers that do that,
        // and why this one is deliberately not on it.
        match self.path.first().copied().flatten() {
            Some(g) => self.option_rows(g),
            None if self.path.is_empty() => self.page(),
            None => Vec::new(),
        }
    }

    fn push_path(&mut self, segment: &str) {
        self.path
            .push(Group::from_label(&tags::strip_display(segment)));
    }

    fn pop_path(&mut self) {
        self.path.pop();
    }

    fn current_path(&self) -> &str {
        if self.path.is_empty() { "/" } else { "/group" }
    }

    fn on_radio_change(&mut self, group: &str, value: &str) {
        let Some(group) = Group::from_label(&tags::strip_display(group)) else {
            return;
        };
        let (labels, selected) = self.options(group);
        let Some(idx) = labels.iter().position(|l| l == value) else {
            return;
        };
        match group {
            Group::User => {
                if idx == selected {
                    return;
                }
                self.selected_user = idx;
                self.message = None;
                // A new conversation: its first question is news again.
                self.last_prompt = None;
                self.dirty = true;
                self.announcement = Some(t_with("login-announce-user", &[("name", value)]));
                // greetd is configuring a session for the *previous* user.
                // Cancel it before asking for another, or the new attempt is
                // refused.
                if let Some(g) = self.greetd.as_ref() {
                    g.send(GreetdCmd::Cancel);
                }
                self.begin_for_selected_user();
            }
            Group::Session => {
                self.selected_session = idx;
                self.announcement = Some(t_with("login-announce-session", &[("name", value)]));
                // No greetd traffic: the session only matters at start_session.
            }
            Group::FontScale | Group::ColorScheme => {
                let (key, stored) = if group == Group::FontScale {
                    (KEY_FONT_SCALE, FONT_SCALES[idx])
                } else {
                    (KEY_COLOR_SCHEME, COLOR_SCHEMES[idx])
                };
                if self.change_setting(key, stored) {
                    self.announcement = Some(t_with(
                        "login-setting-changed",
                        &[("setting", &group.label()), ("value", value)],
                    ));
                }
            }
            Group::Language => {
                // No announcement here: the host switches the locale and
                // announces in the new language, in the new voice.
                self.change_setting(KEY_LANGUAGE, LANGUAGES[idx]);
            }
        }
    }

    fn on_checkbox_change(&mut self, label: &str, checked: bool) {
        let label = tags::strip_display(label);
        let value = if checked { "true" } else { "false" };
        if label == t("login-show-password") {
            // Straight onto the queue, not through `change_setting`: the host
            // reveals the field, and nothing is saved.
            self.show_password = checked;
            self.queue
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push((KEY_SHOW_PASSWORD.to_owned(), value.to_owned()));
            let state = t(if checked { "login-on" } else { "login-off" });
            self.announcement = Some(t_with(
                "login-setting-changed",
                &[("setting", &label), ("value", &state)],
            ));
        } else if label == t("login-setting-screen-reader") {
            // Announced by the host once it knows whether the screen reader
            // actually started.
            self.change_setting(KEY_SCREEN_READER, value);
        } else if label == t("login-setting-shoulder-surfing")
            && self.change_setting(KEY_SHOULDER_SURFING, value)
        {
            let state = t(if checked { "login-on" } else { "login-off" });
            self.announcement = Some(t_with(
                "login-setting-changed",
                &[("setting", &label), ("value", &state)],
            ));
        }
    }

    fn commit_edit(&mut self, _old: &str, new: &str) -> bool {
        // The host hands over the real typed text, not the mask.
        let value = tags::extract_password(new)
            .or_else(|| tags::extract_input(new))
            .unwrap_or_else(|| tags::strip_display(new));

        if let Some(u) = tags::extract_input(new)
            && self.users.is_empty()
        {
            // The typed-name degradation path.
            let name = tags::strip_display(&u);
            if !name.is_empty() {
                self.users = vec![UserEntry {
                    name,
                    uid: 0,
                    full_name: None,
                    shell: String::new(),
                }];
                self.selected_user = 0;
                self.begin_for_selected_user();
            }
            return true;
        }

        self.password = Zeroizing::new(value);
        self.submit();
        true
    }

    fn on_button_press(&mut self, function_name: &str) {
        let Some(line) = power::Commands::announcement(function_name) else {
            return;
        };
        // Say it before spawning, so a screen reader gets the words out while
        // the screen is still up.
        self.announcement = Some(line);
        if power::Commands::ends_the_session(function_name)
            && let Some(g) = self.greetd.as_ref()
        {
            g.send(GreetdCmd::Cancel);
        }
        if let Err(e) = self.power.run(function_name) {
            let action = power::Commands::label(function_name).unwrap_or_default();
            let error = e.to_string();
            self.report(t_with(
                "login-power-failed",
                &[("action", &action), ("error", &error)],
            ));
        }
    }

    fn tick(&mut self) -> bool {
        // The clock deliberately does NOT report through `tick`: a true tick
        // rebuilds the tree even in Insert mode, which would throw away a
        // half-typed password every second. It goes through `needs_refresh`,
        // which the host gates on not being in Insert mode.
        if self.refresh_clock() {
            self.cosmetic = true;
        }
        // A settings file changed under us (an admin's rebuild rewrote /etc):
        // the host applies it like a choice made here, and the page shows it.
        // Cosmetic, like the clock, so a half-typed password survives it.
        for (key, value) in self.settings.poll() {
            self.queue
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push((key.to_owned(), value));
            self.cosmetic = true;
        }
        let changed = self.drain_greetd();
        self.dirty |= changed;
        std::mem::take(&mut self.dirty)
    }

    fn needs_refresh(&self) -> bool {
        self.cosmetic
    }

    fn clear_needs_refresh(&mut self) {
        self.cosmetic = false;
    }

    fn take_announcement(&mut self) -> Option<String> {
        self.announcement.take()
    }

    /// Returned, not taken: the host clears its header on every rebuild, and
    /// with a clock that rebuilds every second a taken error would be gone
    /// after one. The host speaks a header error only when it changes, so
    /// returning it each time does not repeat it.
    fn take_error(&mut self) -> Option<String> {
        self.fatal.clone().or_else(|| self.message.clone())
    }
}

/// `Monday 28 September 2026, 21:04:05`, in local time and the active language.
///
/// Hand-rolled rather than pulling `chrono` in: a greeter needs one format, and
/// the date arithmetic below is the whole of it.
fn format_clock(unix_secs: u64) -> String {
    // 1970-01-01 was a Thursday, which is why this starts there.
    const DAYS: [&str; 7] = [
        "login-day-thursday",
        "login-day-friday",
        "login-day-saturday",
        "login-day-sunday",
        "login-day-monday",
        "login-day-tuesday",
        "login-day-wednesday",
    ];

    let local = unix_secs as i64 + local_utc_offset_secs(unix_secs as i64);
    let days = local.div_euclid(86_400);
    let secs_today = local.rem_euclid(86_400);
    let (h, m, s) = (secs_today / 3600, (secs_today % 3600) / 60, secs_today % 60);

    let weekday = t(DAYS[days.rem_euclid(7) as usize]);
    let (year, month, day) = civil_from_days(days);
    let month = t(&format!("login-month-{month}"));
    t_with(
        "login-clock",
        &[
            ("weekday", &weekday),
            ("day", &day.to_string()),
            ("month", &month),
            ("year", &year.to_string()),
            ("time", &format!("{h:02}:{m:02}:{s:02}")),
        ],
    )
}

/// Seconds east of UTC at `at`, from the `TZ`-aware `localtime_r`.
///
/// Asked for the moment being shown, not for a fixed one: the offset changes
/// with daylight saving time, and asking at the epoch put the clock an hour
/// behind all summer.
fn local_utc_offset_secs(at: i64) -> i64 {
    // SAFETY: `localtime_r` writes into a `tm` we own and reads a `time_t` we
    // own; neither pointer escapes. `tm_gmtoff` is a GNU/BSD extension that
    // libc exposes on every platform this binary is built for (Linux only).
    unsafe {
        let t = at as libc::time_t;
        let mut out: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut out).is_null() {
            return 0;
        }
        out.tm_gmtoff as i64
    }
}

/// Days since the Unix epoch to `(year, month, day)`.
///
/// Howard Hinnant's `civil_from_days`, which is the standard branch-free way to
/// do this and is why there is no leap-year table here.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fakegreetd::{Step, bind, serve_script};
    use crate::greetd::{AuthMessageType, ErrorType, GreetdClient, Response};
    use crate::sessions::SessionType;
    use std::time::{Duration, Instant};

    fn user(name: &str) -> UserEntry {
        UserEntry {
            name: name.to_owned(),
            uid: 1000,
            full_name: None,
            shell: "/bin/sh".to_owned(),
        }
    }

    fn session(id: &str, name: &str) -> SessionEntry {
        SessionEntry {
            id: id.to_owned(),
            name: name.to_owned(),
            exec: vec![format!("/bin/{id}")],
            desktop_names: name.to_owned(),
            session_type: SessionType::Wayland,
        }
    }

    /// A provider with no greetd behind it — enough for every page-shape test.
    fn offline(users: Vec<UserEntry>, sessions: Vec<SessionEntry>) -> LoginProvider {
        let dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        LoginProvider::new(
            users,
            sessions,
            lastlogin::Store::load(dir.path()),
            power::Commands::default(),
            None,
            Settings::load(dir.path(), &dir.path().join("no-defaults.json")),
        )
    }

    const USER: &str = "User";
    const SESSION: &str = "Session";

    fn two_users() -> LoginProvider {
        offline(
            vec![user("nico"), user("guest")],
            vec![
                session("desicompass", "Desicompass"),
                session("cosmic", "COSMIC"),
            ],
        )
    }

    fn labels(v: &[FfonElement]) -> Vec<String> {
        v.iter()
            .map(|e| match e {
                FfonElement::Str(s) => s.clone(),
                FfonElement::Obj(o) => o.key.clone(),
            })
            .collect()
    }

    // ---- Page shape ----

    #[test]
    fn the_page_has_two_radio_groups_a_password_and_three_power_buttons() {
        let mut p = two_users();
        let page = p.fetch();
        let l = labels(&page);

        assert_eq!(l[0], "<radio>User");
        assert_eq!(l[1], "<radio>Session");
        assert!(l[2].starts_with("Password: <password>"), "{}", l[2]);

        // Exactly one password field on the page.
        assert_eq!(
            page.iter()
                .filter(|e| matches!(e, FfonElement::Str(s) if tags::has_password(s)))
                .count(),
            1
        );
        // No login button: Enter in the field is the only way to submit.
        assert!(
            !l.iter().any(|s| s.contains("<button>login")),
            "there must be no login button: {l:?}"
        );
        // Three separate power buttons, not a group.
        for f in [power::SUSPEND, power::REBOOT, power::POWEROFF] {
            assert_eq!(
                l.iter()
                    .filter(|s| s.contains(&format!("<button>{f}</button>")))
                    .count(),
                1,
                "expected exactly one {f} button in {l:?}"
            );
        }
    }

    #[test]
    fn the_password_row_index_points_at_the_password() {
        let mut p = two_users();
        let page = p.fetch();
        let row = p.password_row();
        match &page[row] {
            FfonElement::Str(s) => assert!(tags::has_password(s), "row {row} was {s}"),
            other => panic!("row {row} is not a leaf: {other:?}"),
        }
    }

    /// With no sessions the page is one row shorter, and the index must follow.
    #[test]
    fn the_password_row_index_follows_a_missing_session_group() {
        let mut p = offline(vec![user("nico")], vec![]);
        let page = p.fetch();
        let row = p.password_row();
        match &page[row] {
            FfonElement::Str(s) => assert!(tags::has_password(s), "row {row} was {s}"),
            other => panic!("row {row} is not a leaf: {other:?}"),
        }
    }

    #[test]
    fn the_selected_option_is_the_checked_one() {
        let mut p = two_users();
        let page = p.fetch();
        let group = page[0].as_obj().unwrap();
        assert_eq!(group.children.len(), 2);
        assert!(tags::has_checked(group.children[0].as_str().unwrap()));
        assert!(!tags::has_checked(group.children[1].as_str().unwrap()));
    }

    /// The contract that keeps `refresh_current_directory`'s depth-≥-2 branch
    /// from grafting a copy of the whole page under one of its own children.
    #[test]
    fn fetch_inside_a_group_returns_only_that_groups_options() {
        let mut p = two_users();
        p.push_path("<radio>User");
        let inside = p.fetch();
        assert_eq!(labels(&inside), vec!["<checked>nico", "guest"]);

        p.pop_path();
        p.push_path("<radio>Session");
        let inside = p.fetch();
        assert_eq!(labels(&inside), vec!["<checked>Desicompass", "COSMIC"]);

        p.pop_path();
        assert_eq!(p.fetch().len(), 14, "back to the whole page");
    }

    #[test]
    fn with_no_accounts_the_user_row_becomes_a_typed_input() {
        let mut p = offline(vec![], vec![session("s", "S")]);
        let l = labels(&p.fetch());
        assert!(l[0].starts_with("User: <input>"), "{}", l[0]);
    }

    // ---- Selection ----

    #[test]
    fn changing_the_session_announces_but_sends_nothing() {
        let mut p = two_users();
        p.on_radio_change(SESSION, "COSMIC");
        assert_eq!(p.selected_session, 1);
        assert_eq!(p.take_announcement().as_deref(), Some("Session COSMIC"));
    }

    #[test]
    fn an_unknown_option_is_ignored() {
        let mut p = two_users();
        p.on_radio_change(SESSION, "Plan 9");
        assert_eq!(p.selected_session, 0);
        p.on_radio_change("Nonsense", "nico");
        assert_eq!(p.selected_user, 0);
    }

    // ---- Announcements never leak the password ----

    /// The provider-side counterpart to the host's own guarantee (see the
    /// tests around `input_is_password` in `accesskit_sdl.rs`).
    #[test]
    fn no_announcement_ever_contains_the_password() {
        const SECRET: &str = "correct-horse-battery-staple";
        let mut p = two_users();
        let mut spoken: Vec<String> = Vec::new();

        p.phase = Phase::Prompting { secret: true };
        p.commit_edit("", &tags::format_password(SECRET));
        while let Some(a) = p.take_announcement() {
            spoken.push(a);
        }
        p.tick();
        while let Some(a) = p.take_announcement() {
            spoken.push(a);
        }

        for line in &spoken {
            assert!(
                !line.contains(SECRET),
                "announcement leaked the password: {line}"
            );
            // Not even a fragment of it.
            for w in SECRET.split('-') {
                assert!(!line.contains(w), "announcement leaked {w:?}: {line}");
            }
        }
        assert!(!spoken.is_empty(), "submitting must say something");
    }

    #[test]
    fn the_password_buffer_is_cleared_after_submitting() {
        let mut p = two_users();
        p.phase = Phase::Prompting { secret: true };
        p.commit_edit("", &tags::format_password("hunter2"));
        assert!(
            p.password.is_empty(),
            "the provider must not keep the password after handing it over"
        );
    }

    #[test]
    fn fetch_always_emits_an_empty_password_field() {
        let mut p = two_users();
        p.phase = Phase::Prompting { secret: true };
        p.commit_edit("", &tags::format_password("hunter2"));
        let page = p.fetch();
        let row = page[p.password_row()].as_str().unwrap().to_owned();
        assert_eq!(
            tags::extract_password(&row).as_deref(),
            Some(""),
            "the typed value belongs to the host's insert buffer, not here"
        );
    }

    #[test]
    fn submitting_with_no_prompt_outstanding_sends_nothing_and_says_so() {
        let mut p = two_users();
        assert_eq!(p.phase, Phase::Idle);
        p.commit_edit("", &tags::format_password("hunter2"));
        assert_eq!(p.phase, Phase::Idle, "phase must not advance");
        assert!(
            p.message.is_some(),
            "the user must be told why nothing happened"
        );
    }

    // ---- tick vs needs_refresh ----

    /// Stated as a test so nobody moves the clock onto `tick`. A `true` tick
    /// rebuilds the tree even in Insert mode, and the symptom would be "my
    /// password disappears while I type it, once a minute".
    #[test]
    fn the_clock_refreshes_through_needs_refresh_never_through_tick() {
        let mut p = two_users();
        p.clock = 0; // force the time to look stale

        assert!(!p.tick(), "a clock change must not report through tick()");
        assert!(p.needs_refresh(), "it must report through needs_refresh()");
        p.clear_needs_refresh();
        assert!(!p.needs_refresh());
    }

    #[test]
    fn the_clock_reads_as_a_full_date_and_a_time_with_seconds() {
        // 2026-09-22 was a Tuesday. Rendered in local time, so assert shape
        // rather than an exact string.
        let s = format_clock(1_758_534_000);
        let (date, time) = s.split_once(", ").expect("'date, time'");
        let words: Vec<&str> = date.split(' ').collect();
        assert_eq!(words.len(), 4, "weekday day month year: {date}");
        assert!(words[3] == "2025" || words[3] == "2026", "the year: {date}");
        assert_eq!(time.len(), 8, "HH:MM:SS: {time}");
        assert_eq!(time.matches(':').count(), 2, "HH:MM:SS: {time}");
    }

    /// The old clock asked for the UTC offset in 1970, which put it an hour
    /// behind all summer in a zone with daylight saving time.
    #[test]
    fn the_utc_offset_is_taken_at_the_moment_shown() {
        // Checked against the machine's own zone rather than by setting TZ,
        // which would race with every other test reading the environment. On a
        // machine in Central European Time (the developer's) this is a real
        // check; in UTC (CI) there is no daylight saving time to get wrong.
        let summer = 1_782_864_000; // 2026-07-01
        let winter = 1_767_225_600; // 2026-01-01
        if local_utc_offset_secs(winter) == 3600 {
            assert_eq!(local_utc_offset_secs(summer), 7200);
        }
    }

    #[test]
    fn the_clock_ticks_every_second_but_only_every_minute_while_focused() {
        let mut p = two_users();
        let now = p.clock;

        p.clock = now - 1;
        assert!(p.refresh_clock(), "a second later, unfocused: moves");

        p.clock_focused.store(true, Ordering::Relaxed);
        // Same minute as now, one second behind.
        p.clock = now - (now % 60).min(1);
        if !now.is_multiple_of(60) {
            assert!(
                !p.refresh_clock(),
                "focused, the row must not change within the minute"
            );
        }
        p.clock = now - 60;
        assert!(p.refresh_clock(), "focused, it still moves on the minute");
    }

    // ---- Against a real socket ----

    fn drive(p: &mut LoginProvider, want_phase: Phase) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while p.phase != want_phase && Instant::now() < deadline {
            p.tick();
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn with_greetd(script: Vec<Step>) -> (LoginProvider, std::thread::JoinHandle<Vec<String>>) {
        let dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        let (listener, path) = bind(dir.path());
        let server = std::thread::spawn(move || serve_script(&listener, script));
        let client = GreetdClient::connect_to(path.to_str().unwrap()).unwrap();
        let worker = GreetdWorker::spawn(client);
        let state = tempfile::tempdir().unwrap();
        let p = LoginProvider::new(
            vec![user("nico"), user("guest")],
            vec![session("desicompass", "Desicompass")],
            lastlogin::Store::load(state.path()),
            power::Commands::default(),
            Some(worker),
            Settings::load(state.path(), &state.path().join("no-defaults.json")),
        );
        std::mem::forget(state);
        (p, server)
    }

    #[test]
    fn a_full_login_starts_the_session_and_sets_done() {
        let (mut p, server) = with_greetd(vec![
            Step::new(
                "create_session",
                Response::AuthMessage {
                    auth_message_type: AuthMessageType::Secret,
                    auth_message: "Password:".into(),
                },
            ),
            Step::new("hunter2", Response::Success),
            Step::new("start_session", Response::Success),
        ]);

        drive(&mut p, Phase::Prompting { secret: true });
        p.commit_edit("", &tags::format_password("hunter2"));
        drive(&mut p, Phase::Done);

        assert!(p.is_done());
        assert!(
            p.done_flag().load(Ordering::Relaxed),
            "the loop must be told to stop"
        );

        let seen = server.join().unwrap();
        assert!(
            seen[2].contains(r#""cmd":["/bin/desicompass"]"#),
            "{}",
            seen[2]
        );
        assert!(seen[2].contains("XDG_SESSION_TYPE=wayland"), "{}", seen[2]);
    }

    #[test]
    fn a_wrong_password_reports_and_starts_a_fresh_attempt() {
        let (mut p, server) = with_greetd(vec![
            Step::new(
                "create_session",
                Response::AuthMessage {
                    auth_message_type: AuthMessageType::Secret,
                    auth_message: "Password:".into(),
                },
            ),
            Step::new(
                "wrong",
                Response::Error {
                    error_type: ErrorType::AuthError,
                    description: "authentication error: PERM_DENIED".into(),
                },
            ),
            // greetd keeps the failed session open until it is cancelled...
            Step::new("cancel_session", Response::Success),
            // ...and only then accepts the provider's fresh attempt.
            Step::new(
                "create_session",
                Response::AuthMessage {
                    auth_message_type: AuthMessageType::Secret,
                    auth_message: "Password:".into(),
                },
            ),
        ]);

        drive(&mut p, Phase::Prompting { secret: true });
        p.commit_edit("", &tags::format_password("wrong"));
        drive(&mut p, Phase::Waiting);
        // Let the retry land.
        let deadline = Instant::now() + Duration::from_secs(5);
        while p.message.is_none() && Instant::now() < deadline {
            p.tick();
            std::thread::sleep(Duration::from_millis(2));
        }

        assert_eq!(p.message.as_deref(), Some("Wrong password. Try again."));
        assert_eq!(
            p.take_error().as_deref(),
            Some("Wrong password. Try again."),
            "the failure goes to the header"
        );
        assert!(!p.is_done());
        let seen = server.join().unwrap();
        assert_eq!(
            seen.len(),
            4,
            "a failed attempt must be cancelled and restarted, not left half-open"
        );
        assert!(seen[2].contains("cancel_session"), "{seen:?}");
        assert!(seen[3].contains("create_session"), "{seen:?}");
    }

    #[test]
    fn switching_user_cancels_the_session_that_was_being_configured() {
        let (mut p, server) = with_greetd(vec![
            Step::new(
                "create_session",
                Response::AuthMessage {
                    auth_message_type: AuthMessageType::Secret,
                    auth_message: "Password:".into(),
                },
            ),
            Step::new("cancel_session", Response::Success),
            Step::new(
                "create_session",
                Response::AuthMessage {
                    auth_message_type: AuthMessageType::Secret,
                    auth_message: "Password:".into(),
                },
            ),
        ]);

        drive(&mut p, Phase::Prompting { secret: true });
        p.on_radio_change(USER, "guest");
        drive(&mut p, Phase::Prompting { secret: true });

        assert_eq!(p.selected_user, 1);
        let seen = server.join().unwrap();
        assert!(seen[1].contains("cancel_session"), "got {}", seen[1]);
        assert!(seen[2].contains(r#""username":"guest""#), "got {}", seen[2]);
    }

    /// The bug a user hit at the real login screen: a wrong password, then the
    /// right one, and no session. Driven against the stateful fake, which
    /// refuses a new session while a failed one is still open, as greetd does.
    #[test]
    fn a_wrong_password_then_the_right_one_logs_in() {
        let dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        let (listener, path) = bind(dir.path());
        std::thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let _ = crate::fakegreetd::converse(&mut conn, "hunter2");
        });
        let client = GreetdClient::connect_to(path.to_str().unwrap()).unwrap();
        let mut p = LoginProvider::new(
            vec![user("nico")],
            vec![session("desicompass", "Desicompass")],
            lastlogin::Store::load(dir.path()),
            power::Commands::default(),
            Some(GreetdWorker::spawn(client)),
            Settings::load(dir.path(), &dir.path().join("no-defaults.json")),
        );

        drive(&mut p, Phase::Prompting { secret: true });
        p.commit_edit("", &tags::format_password("wrong"));
        drive(&mut p, Phase::Waiting);
        drive(&mut p, Phase::Prompting { secret: true });
        assert_eq!(
            p.phase,
            Phase::Prompting { secret: true },
            "after a wrong password greetd must be asking again; message: {:?}",
            p.message
        );

        p.commit_edit("", &tags::format_password("hunter2"));
        drive(&mut p, Phase::Done);
        assert!(p.is_done(), "the right password must start the session");
    }

    // ---- Only the show-password box below the password field ----

    #[test]
    fn greetds_prompt_is_announced_but_not_put_on_the_page() {
        let (mut p, _server) = with_greetd(vec![Step::new(
            "create_session",
            Response::AuthMessage {
                auth_message_type: AuthMessageType::Secret,
                auth_message: "Password:".into(),
            },
        )]);
        drive(&mut p, Phase::Prompting { secret: true });
        assert_eq!(p.take_announcement().as_deref(), Some("Password:"));
        let l = labels(&p.fetch());
        assert!(
            !l.iter().any(|s| s.contains("Password:") && !tags::has_password(s)),
            "greetd's prompt must not be on the page: {l:?}"
        );
        let below = &l[p.password_row() + 1];
        assert_eq!(
            below, "<checkbox>show password",
            "the row under the field must be the show-password box"
        );
    }

    /// After a wrong password greetd asks "Password:" again a moment later.
    /// Announcing that replaced "Wrong password" in the live region before the
    /// screen reader had said it.
    #[test]
    fn the_same_prompt_asked_again_is_not_announced_over_the_failure() {
        let secret = || Response::AuthMessage {
            auth_message_type: AuthMessageType::Secret,
            auth_message: "Password:".into(),
        };
        let (mut p, _server) = with_greetd(vec![
            Step::new("create_session", secret()),
            Step::new(
                "wrong",
                Response::Error {
                    error_type: ErrorType::AuthError,
                    description: "authentication error: PERM_DENIED".into(),
                },
            ),
            Step::new("cancel_session", Response::Success),
            Step::new("create_session", secret()),
        ]);

        drive(&mut p, Phase::Prompting { secret: true });
        assert_eq!(p.take_announcement().as_deref(), Some("Password:"));

        p.commit_edit("", &tags::format_password("wrong"));
        let mut spoken = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(5);
        // Until the retry's prompt has arrived.
        while !(p.message.is_some() && p.phase == (Phase::Prompting { secret: true }))
            && Instant::now() < deadline
        {
            p.tick();
            if let Some(a) = p.take_announcement() {
                spoken.push(a);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(
            p.phase,
            Phase::Prompting { secret: true },
            "the retry arrived"
        );
        assert!(
            !spoken.iter().any(|a| a == "Password:"),
            "the repeated prompt must not be announced: {spoken:?}"
        );
        assert_eq!(
            p.take_error().as_deref(),
            Some("Wrong password. Try again."),
            "the header says what happened"
        );
    }

    #[test]
    fn a_failure_stays_in_the_header_until_the_next_attempt() {
        let mut p = two_users();
        p.commit_edit("", &tags::format_password("x"));
        let first = p.take_error();
        assert!(first.is_some(), "a refused submit must be reported");
        assert_eq!(p.take_error(), first, "asking again must not lose it");
        assert!(
            !labels(&p.fetch()).iter().any(|l| Some(l) == first.as_ref()),
            "it goes to the header, not onto the page"
        );

        p.on_radio_change(USER, "guest");
        // Offline, so the new attempt reports "not connected"; what matters is
        // that the old failure is gone.
        assert_ne!(p.take_error(), first);
    }

    // ---- Show password ----

    #[test]
    fn the_show_password_box_sits_under_the_field_unticked() {
        let mut p = two_users();
        let l = labels(&p.fetch());
        assert_eq!(l[p.password_row() + 1], "<checkbox>show password");
        assert!(l[p.password_row() + 2].starts_with("<button>"), "{l:?}");
    }

    #[test]
    fn ticking_show_password_tells_the_host_and_keeps_the_field_a_password() {
        let mut p = two_users();
        p.on_checkbox_change("show password", true);
        assert_eq!(
            drain(&p),
            vec![("showPassword".to_owned(), "true".to_owned())]
        );
        assert_eq!(
            p.take_announcement().as_deref(),
            Some("show password: on")
        );
        let page = p.fetch();
        let l = labels(&page);
        assert_eq!(l[p.password_row() + 1], "<checkbox checked>show password");
        assert!(
            tags::has_password(&l[p.password_row()]),
            "the field stays a password; revealing it is the renderer's: {}",
            l[p.password_row()]
        );

        p.on_checkbox_change("show password", false);
        assert_eq!(
            drain(&p),
            vec![("showPassword".to_owned(), "false".to_owned())]
        );
        assert_eq!(
            labels(&p.fetch())[p.password_row() + 1],
            "<checkbox>show password"
        );
    }

    // ---- Settings ----

    #[test]
    fn the_settings_follow_the_clock_flat_on_the_page() {
        let mut p = two_users();
        let page = p.fetch();
        let l = labels(&page);
        let c = p.clock_row();
        assert!(l[c].contains(':'), "the clock row: {}", l[c]);
        assert_eq!(l[c + 1], "<checkbox checked>screen reader");
        assert_eq!(l[c + 2], "<radio>font scale");
        assert_eq!(l[c + 3], "<radio>color scheme");
        assert_eq!(l[c + 4], "<radio>language");
        assert_eq!(
            l[c + 5],
            "<checkbox>shoulder-surfing protection (blank screen)"
        );
        assert_eq!(
            l[c + 6],
            format!("loginsicompass version: {}", env!("CARGO_PKG_VERSION")),
            "the version is the last row"
        );
        assert_eq!(l.len(), c + 7);
    }

    #[test]
    fn the_language_options_are_each_in_their_own_language() {
        let mut p = two_users();
        p.push_path("<radio>language");
        assert_eq!(
            labels(&p.fetch()),
            vec![
                "<checked>English",
                "Nederlands (België)",
                "Français (Belgique)",
                "Deutsch (Belgien)"
            ]
        );
    }

    fn drain(p: &LoginProvider) -> Vec<(String, String)> {
        std::mem::take(&mut *p.settings_queue().lock().unwrap())
    }

    #[test]
    fn choosing_a_font_scale_saves_queues_and_announces() {
        let mut p = two_users();
        p.on_radio_change("<radio>font scale", "2.00");
        assert_eq!(drain(&p), vec![("fontScale".to_owned(), "2.00".to_owned())]);
        assert_eq!(p.settings.font_scale(), "2.00");
        assert_eq!(p.take_announcement().as_deref(), Some("font scale: 2.00"));
    }

    #[test]
    fn choosing_a_color_scheme_stores_the_neutral_value() {
        let mut p = two_users();
        p.on_radio_change("color scheme", "light");
        assert_eq!(
            drain(&p),
            vec![("colorScheme".to_owned(), "light".to_owned())]
        );
    }

    #[test]
    fn choosing_a_language_queues_its_locale_and_leaves_the_words_to_the_host() {
        let mut p = two_users();
        p.on_radio_change("language", "Nederlands (België)");
        assert_eq!(drain(&p), vec![("language".to_owned(), "nl-BE".to_owned())]);
        assert_eq!(p.take_announcement(), None);
    }

    #[test]
    fn unticking_the_screen_reader_is_queued_for_the_host() {
        let mut p = two_users();
        p.on_checkbox_change("screen reader", false);
        assert_eq!(
            drain(&p),
            vec![("screenReader".to_owned(), "false".to_owned())]
        );
        assert!(!p.settings.screen_reader());
        let l = labels(&p.fetch());
        assert!(l.contains(&"<checkbox>screen reader".to_owned()), "{l:?}");
    }

    #[test]
    fn an_unknown_setting_value_changes_nothing() {
        let mut p = two_users();
        p.on_radio_change("font scale", "9.99");
        p.on_checkbox_change("nonsense", true);
        assert!(drain(&p).is_empty());
    }

    /// The path holds which group, not its label: a language change made from
    /// inside a group must not strand `fetch` on a label that no longer exists.
    #[test]
    fn the_path_remembers_the_group_not_its_label() {
        let mut p = two_users();
        p.push_path("<radio>font scale");
        assert_eq!(p.path, vec![Some(Group::FontScale)]);
        assert_eq!(p.fetch().len(), FONT_SCALES.len());
        p.pop_path();
        p.push_path("<radio>no such group");
        assert!(p.fetch().is_empty());
        p.pop_path();
        assert!(p.path.is_empty());
    }
}
