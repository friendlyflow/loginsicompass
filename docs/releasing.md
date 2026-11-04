# Releasing

How a `vX.Y.Z` tag turns into installable packages. The steps to follow are the
`/release loginsicompass` skill. This is the background.

## What a release produces

One GitHub Release carrying:

| Asset | Built by | For |
| --- | --- | --- |
| `loginsicompass_amd64.deb` | cargo-deb | Debian 12, Ubuntu 24.04 and newer |
| `loginsicompass.x86_64.rpm` | cargo-generate-rpm | Fedora, openSUSE |
| `loginsicompass-x86_64-unknown-linux-gnu.tar.xz` | `tar` | anyone installing by hand |
| `SHA256SUMS.txt` | `sha256sum` | verification |

Each holds `loginsicompass`, the script greetd starts, and the license texts. The
floor of the `.deb` is greetd's: Ubuntu 22.04 does not package it, even though
the binary runs there. Nix is the other format and is not an asset: the flake
builds from the tag, and desicompass's module installs it.

**The packages install the greeter and do not choose it.** They never write
`/etc/greetd/config.toml` or touch a display manager. A login screen that fails
to start leaves no graphical way in, so switching stays the administrator's step
and the README spells it out. It is step two of the NixOS module's two steps,
after the desicompass session is known to work.

## The pipeline

```
git push origin vX.Y.Z
   -> .github/workflows/release.yml
      |- packages        on ubuntu-22.04: scripts/setup-freetype.sh,
      |                  scripts/release-package.sh, scripts/release-notes.sh
      |- install-check   in a bare ubuntu:24.04: scripts/check-install.sh
      `- publish         gh release create, gh release upload
```

**Started by hand** (Actions -> Release -> Run workflow, or
`gh workflow run release.yml`) it runs everything but `publish`, and the
files stay on the run as an artifact. CI builds in the flake's toolchain, so this
is the only check that the Ubuntu build still works. The release skill runs it
before tagging.

`scripts/release-package.sh` refuses a tag that is not the version in
`Cargo.toml`, and `ldd`s the binary before packaging it. It stops on a library
the runner lacks, on SDL3 or FreeType linked dynamically, and on a binary without
libpng, which means a FreeType with no PNG support and no colour emoji.

`scripts/check-install.sh` then installs the `.deb` with `--no-install-recommends`
in a bare container, next to the latest desicompass and sicompass, and starts the
session script's command nested in Xvfb on software Vulkan. It needs the GPU greeter
to reach the login page, and nothing is published otherwise. A container and not the
runner, because the runner, like every desktop, already has the libraries that are
the question (SDL3 dlopens libwayland-client). It skips the start with a notice
while desicompass has no released package. It cannot test `--backend tty`, or a real
login through greetd, so those stay in the smoke test.

Why this differs from sicompass's pipeline (no cargo-dist, cargo-deb, no AppImage,
no version in the file names) is in desicompass's `docs/releasing.md`, which
works the same way.

## What the packages assume

`[package.metadata.deb]` and `[package.metadata.generate-rpm]` in `Cargo.toml`
list the files, and the comments above them explain the dependencies.
Things no compiler checks:

- **The session script mirrors the NixOS module.**
  `packaging/loginsicompass-session` is `services.greetd` in desicompass's
  `flake.nix` without the store paths. A change to one belongs in the other, and
  desicompass's `CLAUDE.md` forbids passing a flag an older compositor lacks.
  `tests/packaging.rs` holds the load-bearing words.
- **The greetd account's name depends on the distribution.**
  `packaging/scripts/postinst` tries `_greetd` (Debian, Ubuntu), `greetd` (Fedora)
  and `greeter` (Arch, NixOS), gives it `/var/lib/loginsicompass`, and adds it to
  `sicompass-a11y`, the group desicompass's package creates.
- **Dlopened libraries have to be declared by hand.** The Vulkan loader and three
  libwayland libraries show up in no scan.
- **The `.rpm`'s requirements come from `packaging/rpm-find-requires`,** because
  cargo-generate-rpm's own scan uses `ldd`, which also lists what the libraries
  link, and Ubuntu's are not Fedora's.
- **`scripts/setup-freetype.sh` is a copy** of the Linux half of sicompass's.
- **`release.yml` names the build's Ubuntu packages**, and nothing else does.

## Smoke test

A green run proves that a clean install starts nested. Check the real thing on a
virtual machine with a GPU, or a computer with a second way in:

- Install it with desicompass, follow the README's steps to switch, and reboot.
  The login screen should appear and speak, and logging in should start the
  session you picked.
- `journalctl -t loginsicompass -b` holds its output.
- `ls -ld /var/lib/loginsicompass` belongs to the greetd account, and the user and
  session are remembered after a reboot.
- A font scale chosen on the login screen is what the session starts with
  (`/var/lib/sicompass/accessibility.json`).
- Without a GPU the login screen restarts in its software mode. It should still
  log in.
