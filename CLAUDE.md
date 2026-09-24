# Project Instructions

loginsicompass was split out of the
[sicompass](https://github.com/friendlyflow/sicompass) workspace. Its git
history before that point is the history of `src/loginsicompass` there (before
a rename, `src/loginsicompass-rs` for the Rust port, and `src/loginsicompass`
for the C original, which later moved to `legacy-c/`). Work on it is usually
driven from a sicompass checkout next to this one (`../sicompass`), whose
`/commit-and-push`, `/release`, `/sync` and `/update-cargo` take this repo's
name as their first argument and then follow the skills in this repo's
`.claude/skills/`.

## Environment (Nix)

The toolchain comes from the flake dev shell in [flake.nix](flake.nix). Nothing
is installed system-wide.

- **Check once per session**, then stick with the answer: `command -v cargo`.
  - Non-empty: the shell is inside `nix develop`, so run `cargo ...` directly.
  - Empty: prefix every toolchain command with `nix develop -c`.
- `nix develop -c <cmd>` prints a `warning: Git tree ... is dirty` line on
  stderr first. That warning is noise, not a failure.
- Evaluate the flake through `git+file://$PWD`, never a plain path (a plain path
  copies `target/` into the store and hangs), and always under `timeout`.
- The version lives in `[package] version` in `Cargo.toml`. `flake.nix` reads it
  from there, so there is only one version to bump.
- Linux only. The dev shell is sicompass-ui's (a flake input), plus `orca` and
  two Wayland test clients.
- `.cargo/config.toml` points every cargo-spawned process at a throwaway XDG
  tree under `target/`. Keep it: this crate links `sicompass-sdk`, whose
  `platform` module would otherwise have tests use the real directories.

## Generated files that are committed

- `fonts/LICENSE-*.txt` are copies of sicompass-ui's. The fonts are compiled
  into this binary through sicompass-ui, so the package installs their
  licenses. `tests/packaging.rs` fails when a moved sicompass-ui pin brings
  different texts: copy them over again.

- `THIRD-PARTY-LICENSES.html`: `cargo about generate about.hbs -o
  THIRD-PARTY-LICENSES.html` (cargo-about 0.9.2, the version the `licenses.yml`
  workflow pins). Regenerate and commit it with any dependency change. The
  workflow fails if it drifts.

## Architecture: the greeter

See [docs/greeter.md](docs/greeter.md) for the page it shows, the greetd
conversation (including the native-byte-order framing that must not regress),
user and session enumeration, the password's lifetime, the GPU-failure
supervisor, and how to run the whole thing nested without touching the boot
path.

**It must never link the `sicompass` application crate.** It draws with
`sicompass-ui`, the renderer it shares with the app, and gets its content
through `sicompass-sdk`'s `Provider`. Linking the app would drag wasmtime, a
bundled SQLite, a headless-Chromium driver and an IMAP/SMTP stack into a login
screen, which is what the renderer was split out to prevent. What the renderer
needs from an embedder it asks through `registry::HostHooks`, and the greeter
takes the defaults.

`sicompass-ui` is a git dependency (`rev` between releases, `tag` at a
release). To work on the renderer and the greeter together, uncomment the
`[patch."https://github.com/friendlyflow/sicompass-ui"]` section at the bottom
of `Cargo.toml`, and comment it out again before committing. Keep
`sicompass-sdk` at the version sicompass-ui requires, or cargo builds two copies
and the `Provider` types stop matching.

The NixOS module that installs the greeter (greetd, the tmpfiles state
directory, the XDG variables, `systemd-cat` and `dbus-run-session`) lives in the
desicompass repo, next to the compositor the greeter runs in.

## Code Style

Follow standard Rust idioms. Use `#[allow(...)]` sparingly and only when
justified. In `README.md`, do not use em dashes or semicolons. Use commas
instead, or split into separate sentences.

## Testing

- `xvfb-run -a cargo test` on a machine without a display, as CI does.
  `tests/greeter_ui.rs` drives the real renderer headlessly.
- `cargo clippy --all-targets`. Dead code carried over from the C port is not
  denied yet, so do not add new warnings.
- A change in behaviour is checked in the nested run from docs/greeter.md,
  with `examples/fake-greetd.rs` standing in for greetd.
- After implementing changes, always run the tests before finishing.
- When adding new code, write or update tests.
- If tests fail, fix the code. Never leave a task with failing tests.

## Test Integrity

- Never remove or weaken test assertions to make a failing test pass. Fix the
  code instead.
- If a test itself is genuinely wrong and needs changing, **ask the user
  first** before modifying it.

## Releasing

A release is a `vX.Y.Z` tag on `main`. See `.claude/skills/release/SKILL.md`.
