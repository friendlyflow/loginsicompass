# The login greeter

loginsicompass is the greetd greeter. It draws the login screen with the
application's own renderer, so a screen-reader user meets the same list, the
same prefixes and the same masked field at the login prompt as inside the app.

It is a separate binary, not a mode of `sicompass`. It links `sicompass-ui` and
deliberately not `sicompass` — see [The crate split](#the-crate-split).

```
greetd
  └─ desicompass --backend tty --startup-cmd loginsicompass-start
       └─ loginsicompass                      supervisor
            └─ loginsicompass --render-backend gpu   sicompass-ui, Vulkan
                 or, if that dies,
               loginsicompass --render-backend shm   tiny-skia, last resort
```

## The page

One provider, `LoginProvider`, whose `fetch()` is the whole screen:

```
+R User [nico]                    ← radio group, names its own selection
+R Session [Desicompass]
-i Password:                      ← cursor lands here at startup
-b Suspend
-b Restart
-b Shut down
-  Monday 28 September 2026, 15:04:05
-c screen reader                  ← the settings, flat under the clock
+R font scale [1.75]
+R color scheme [dark]
+R language [English]
-  shoulder-surfing protection (blank screen)
-  loginsicompass version: 0.2.0
```

**Nothing sits under the password field.** greetd's prompt is *announced*, not
shown: with an ordinary PAM stack it is "Password:", which only repeats the
field's label, and on an unusual one (a 2FA code) it is still heard. Failures
and notices ("Wrong password. Try again.", PAM's own messages) go to the
renderer's header error line through `take_error`. The provider returns the
message rather than taking it, because the renderer clears the header on every
list rebuild, and the clock rebuilds every second. The renderer drains it after
a rebuild, and `GreeterHooks` puts it back every frame after the keys are
handled, since entering or leaving a group rebuilds too. The header speaks an
error once, when it changes, so it must never flicker empty. greetd's second
"Password:" after a wrong one is not announced, or it would replace "Wrong
password" before the screen reader had said it.

**The clock** shows seconds, except while the cursor is on it. There it moves
on the minute: a focused row whose label changes is read out again, and a
screen reader reading the time every second would drown everything else. The
UTC offset is taken for the moment shown (it used to be taken for 1970, which
put the clock an hour behind all summer).

**There is no login button.** Enter in Insert mode on the password field
submits, which is how every other `<input>` in the app commits. A button would
be a second way to do one thing, and would sit between the field and the error
it produces.

The radio groups are served **path-scoped**: `fetch()` returns the whole page at
`/` and only that group's options at `/User`. Do not add `"login"` to the
`whole_tree` list in sicompass-ui's `src/provider.rs` — returning the whole tree
from inside a group grafts a copy of the page under one of its own descendants.

## The crate split

`sicompass-ui` holds the renderer; `sicompass` holds everything only an
*application* has. Linking the application into a login screen cost 465 crates —
wasmtime, a bundled SQLite, a headless-Chromium driver, an IMAP and an SMTP
client, none of them ever called. The greeter links 302.

The rule, enforced by the Stop hook in the sicompass-ui repo: **`sicompass-ui`
must not depend on `sicompass-builtins`, `sicompass-updater`, `wasmtime` or
`reqwest`.** Where the renderer needs an answer only the embedder has, it asks:

| | |
|---|---|
| `registry::HostHooks` | settings, the updater, per-tab provider sets, and when to stop. Eight methods, all defaulting to a no-op — which is correct for the greeter. |
| `http::register_body_fetcher` | an HTTP client for `<link>` and URL `<image>` values. Unregistered, a link reports that it cannot be followed and the node still renders. |
| `app_state::AppConfig` | everything the window used to hardcode. `Default` reproduces the application exactly, and a test asserts it. |

`AppState` lives in `sicompass-ui`, so Rust's orphan rule stops the app adding
an inherent `AppState::new()`. Application startup is `boot::app_state()`.

## Talking to greetd

`greetd.rs` speaks the wire format from `greetd-ipc(7)`: a 32-bit length in
**native byte order**, then JSON. Native, not big-endian — an earlier revision
got this wrong and the greeter had never once completed a real authentication,
because greetd read `00 00 00 2c` as a 738 MB frame and waited. There is a test
pinned to the man page's published hexdump; do not loosen it.

`start_session` takes an **argv and an environment**, not a command line. A
session's `Exec=` is ten words on a NixOS host, and greetd would otherwise
`execve` a file whose name is the whole line. The environment is where
`XDG_SESSION_TYPE`, `XDG_SESSION_DESKTOP` and `XDG_CURRENT_DESKTOP` come from;
without them the session comes up subtly wrong.

`auth.rs` runs the conversation on its own thread, because PAM can take seconds
and the UI thread must keep drawing and keep talking to the screen reader.
`info` and `error` auth messages are acknowledged **inside the worker**: that is
a protocol obligation with no decision in it, and greetd waits forever without
it.

After any `error` response the worker also sends **`cancel_session`**, before it
reads the UI's next command. greetd keeps a failed session open and refuses a
new `create_session` while it is, so without the cancel a wrong password left
the greeter unable to log anyone in until it was restarted: the retry was
refused, and the right password then had no prompt to answer. The fake greetd
(`fakegreetd::converse`, used by `examples/fake-greetd.rs` and by
`a_wrong_password_then_the_right_one_logs_in`) refuses a second session the
same way, so a nested run reproduces it.

## Settings and the screen reader

The rows under the clock are the app's accessibility settings, minus the
update check: screen reader, font scale, color scheme, language and
shoulder-surfing protection. They use the app's own keys and wording.

- **Saved** to `/var/lib/sicompass/accessibility.json`, a flat object holding
  only what was chosen. It is one object for the whole machine, shared both
  ways with every desicompass session: sicompass and desicompass-superkey read
  and write it too. What is chosen here is what the session starts with, and
  what is chosen in a session is what this screen shows next time. The NixOS
  module makes the directory writable by a `sicompass-a11y` group holding the
  greeter and the users (setgid, and writers replace the file by a rename).
  Choices the greeter kept in `<state-dir>/settings.json` before this are moved
  over once, while the shared object has none.
- **Under that**, the system defaults in `/etc/sicompass/accessibility.json`
  (`--defaults-file`), which sicompass reads too. See [System
  defaults](#system-defaults).
- **Under that**, the built-in defaults. They are the app's, except that
  `screenReader` is **on**: the first time the greeter runs nobody has chosen
  anything, and a blind user cannot turn on a screen reader they cannot hear.
  Once someone unticks it, that is saved and the greeter stays quiet.

Both files are read through `sicompass_ui::accessibility::SharedAccessibility`,
the same code the session uses. A save is an atomic rename under a lock, and
the provider polls both files from `tick()`, so a change made while the greeter
runs (an admin's `nixos-rebuild switch` rewriting `/etc`) is applied and shown
as if it had been chosen here. It is applied as a cosmetic refresh, like the
clock, so it never throws away a half-typed password.

The provider saves a change and queues it. `gui::GreeterHooks` applies it:
the display settings through `sicompass_ui::accessibility::apply_display` (the
same code the app uses), the language by switching the locale and rebuilding
the whole page in it, and the screen reader by starting or stopping Orca
(`--screen-reader-command`, `orca` on PATH by default, started with
`--replace`).

Orca is started **before the window**. It looks for the focused window once,
at the end of its startup, and at the login screen the greeter is usually not
registered yet: Orca's log says "Unable to find active window from []", and it
then says nothing but "Screen reader on". A focus event the window sends while
Orca is still starting is lost too.

So the greeter learns when Orca is ready. It hands Orca a socket of its own in
`NOTIFY_SOCKET` (systemd's `sd_notify` protocol, which Orca speaks), and Orca
sends `READY=1` there at the very end of its startup. The greeter answers with
a window focus toggle (`AppRenderer::a11y_refocus_now`), and Orca reads the
window name, the header and the focused row, with no key press. If Orca did
find the window by itself, it reads them twice. Guessing whether it did would
risk the silence coming back. `a11y_refocus_on_move` stays as the fallback for
a screen reader that never reports ready: the first cursor move toggles focus
instead.

**Reproducing it nested** needs three things the login screen has and a desktop
session does not. Unset `XDG_CURRENT_DESKTOP` (and the other session
variables), or the desktop portal tries the desktop's own backend and SDL
blocks on it before creating the window. Unset `DCONF_PROFILE`. And the
accessibility bus launcher must use plain `dbus-daemon`: it picks `dbus-broker`
whenever it runs inside a systemd *user* unit (`sd_pid_get_user_unit`), and
dbus-broker then asks the desktop's systemd to start the registry, which fails
("unit failed") and leaves Orca with no applications at all. The login screen
runs in a system scope, so it never hits this.

Unticking **screen reader** sends Orca `SIGTERM`, not `SIGKILL`. Orca answers
that by saying "screen reader off" itself and exiting. A background thread
reaps it and kills it only if it is still there after six seconds. When the
greeter exits for a session, Orca is stopped at once and quietly, before greetd
hands the display over.

The strings are in `locales/*.ftl`, one per language the app offers, with
`login-` keys because they share the renderer's localizer. A test fails if a
key is missing from any of the four.

## System defaults

`/etc/sicompass/accessibility.json` holds the machine's accessibility
defaults. The greeter, sicompass and desicompass-superkey read it, and all
use it only for what nobody has chosen: anything saved in the shared object,
`/var/lib/sicompass/accessibility.json`, wins over it. Every key is
optional, and nothing writes it at runtime.

| Key | Values |
|---|---|
| `screenReader` | `true`, `false` |
| `fontScale` | `"1.00"` to `"2.50"` in steps of 0.25 |
| `colorScheme` | `"dark"`, `"light"` |
| `language` | `"en-US"`, `"nl-BE"`, `"fr-BE"`, `"de-BE"` |
| `shoulderSurfingProtection` | `true`, `false` |

On NixOS, `services.desicompass.accessibility.*` writes it. Anywhere else, write
it by hand (or ship it in the distribution's package):

```json
{ "screenReader": true, "fontScale": "2.00", "language": "nl-BE" }
```

A distribution package also has to create `/var/lib/loginsicompass` owned by
the greeter user, and `/var/lib/sicompass` (mode 2775) owned by a group holding
the greeter and the users, and install Orca and speech-dispatcher.

## Enumeration

No privileged helper. greetd owns PAM, so nothing here reads `/etc/shadow`, and
both sources are world-readable.

- **Users** — `/etc/passwd`, bounded by `UID_MIN`/`UID_MAX` from
  `/etc/login.defs` and filtered by login shell. Both defences matter and are
  tested separately: a NixOS host has 32 `nixbld` accounts above UID 1000, and
  `UID_MAX 29999` is what keeps them off the login screen.
- **Sessions** — `.desktop` files under `$XDG_DATA_DIRS/{wayland-sessions,
  xsessions}`, plus `/run/current-system/sw/share` unconditionally, plus
  `--sessions-dir`. The desktop-file **id** is the stable key; `Name=` is
  locale-dependent and is display only.

The remembered user and session are committed **only after `start_session`
succeeds**. Persisting on selection would make a mistyped username the
remembered default.

## The password

Masking is the renderer's, not the greeter's: `<password>` already masks the
drawn row, the per-keystroke spoken echo, the spoken context and in-field
search, and `accesskit_sdl.rs` has tests forbidding the value ever being spoken.

Orca hears every key, through desicompass's `org.freedesktop.a11y.KeyboardMonitor`,
and by default echoes typed letters. The one thing that stops it is the focused
object having the password-text role. So the renderer gives the row
`Role::PasswordInput` while a password is being typed (Insert mode on a
`<password>` row), and Orca does not echo. Only then: resting on the row, Orca
would add "password text" to the row's own "dash i Password:". Nothing reaches the
field outside Insert mode, and the role change reaches Orca a frame after Insert
mode starts, before a first character can be typed. Removing that role would make
Orca read passwords aloud.

The buffer is zeroized — bytes wiped, not just the length reset — on every path
that ends a password edit. What that does *not* buy, stated so nobody assumes
more: the live FFON element holds the typed value while the field is being
edited (that is how the provider is handed it), the OS may have paged it out,
and PAM keeps its own copy. It closes the "same process, freed allocation"
window.

## When the GPU path fails

A login screen that does not appear leaves no graphical way into the machine,
and the Vulkan path can fail in ways that are not a `Result`: `.expect`s on
swapchain recreation, a driver taking `SIGSEGV`, SDL calling `abort()`.
`catch_unwind` covers the panics and nothing else.

So the process greetd starts is a supervisor. It re-execs itself as the GPU
greeter and, once, as the software fallback if that dies without having started
a session. It watches a **pipe, not the exit status**: greetd tears the greeter
down the instant `start_session` succeeds, so a successful child is often
`SIGKILL`ed a millisecond later and is otherwise indistinguishable from a crash.

`supervisor::decide` is pure and has the policy in one place. The fallback
(`--render-backend shm`) has no text, no accessibility and no pickers — it is a
way to get in and fix things, not a greeter anyone should meet twice.

Because it draws no text it cannot *show* a picker, so it has to be told who to
log in and what to start. It resolves that through the same enumeration the
graphical greeter uses — the remembered user and session, else the first of
each. It used to read `--user` and `--command`, whose defaults were `nobody` and
`false`; falling back should change how the login screen looks, not who it logs
in.

## Running it without touching the boot path

Nothing below needs `services.desicompass.greeter.enable`. That switch is the
last thing turned on.

```sh
# A greetd that always asks for a password and accepts the one you name.
# The socket path must be short: sockaddr_un caps it at ~108 bytes.
cargo run --example fake-greetd -- /tmp/greetd.sock hunter2

# The greeter, nested inside desicompass (its own repo, checked out next to
# this one at ../desicompass), inside your current session.
cargo build
GREETD_SOCK=/tmp/greetd.sock cargo run --manifest-path ../desicompass/Cargo.toml -- \
  --backend auto \
  --startup-cmd "$PWD/target/debug/loginsicompass --state-dir /tmp/lsc-state"
```

A fresh `--state-dir` is a first run, so the greeter starts Orca, and nested it
shares your desktop's session bus: `orca --replace` takes over the Orca you may
already be running, and stopping it when the greeter exits leaves you without
one. If you rely on a screen reader, add `--screen-reader-command true` (a
command that exits at once) and keep your own. To test the greeter's own Orca,
also pass a `--defaults-file` of your own, so the run does not read
`/etc/sicompass/accessibility.json`.

To prove the fallback rather than assume it, add
`SICOMPASS_FORCE_VULKAN_FAILURE=1` (debug builds only) and watch the log go
`starting the gpu greeter` → `falling back to the software renderer` →
`starting the shm greeter`.

Then, in order, and only then:

1. `services.desicompass.enable = true` — adds the session, leaves greetd alone.
   A failure costs one logout.
2. `services.desicompass.greeter.enable = true` — replaces the greeter.
   `nixos-rebuild build` before `switch`, a root shell already open on another
   VT, and `journalctl -t loginsicompass -b` afterwards to see which renderer it
   took.

`greeter.enable` asserts `enable`, because everything else is gated on the
latter and "I turned it on and nothing happened" is a bad way to learn that
about a login screen.

## What the NixOS module has to get right

The module lives in the desicompass repo's `flake.nix`
(github:friendlyflow/desicompass), next to the compositor it starts, and takes
the `loginsicompass` package from this repo's flake
(`services.desicompass.greeter.package` overrides it).

- **`dbus-run-session`.** `accesskit_unix` speaks AT-SPI2 over the *session*
  bus. Without one the greeter stalls 400ms waiting for a registration that
  never arrives and is then mute to Orca — for an accessibility-first shell that
  is a failure, not a degradation. (COSMIC's own greeter does not do this, and
  its screen-reader toggle is consequently cosmetic on NixOS.)
- **`systemd-cat`.** greetd captures neither stdout nor stderr of what it
  starts, so a greeter that fell back — or failed twice — would say so to
  nobody.
- **A writable state directory.** The greeter user's home is `/var/empty`, so
  `XDG_CONFIG_HOME` and friends point into a tmpfiles-created
  `/var/lib/loginsicompass/xdg`. `sicompass_sdk::platform` honours those ahead
  of `$HOME`.
- **No `--user` or `--command`.** Those flags are what made the old greeter
  authenticate `nobody` and then launch `false`.
- **Orca and speech-dispatcher.** `services.orca.enable` (which brings
  speech-dispatcher), and `--screen-reader-command` with Orca's store path, so
  the greeter does not depend on its `PATH`. The greeter user also needs sound:
  speech-dispatcher plays through the PipeWire of the greeter's own logind
  session.
