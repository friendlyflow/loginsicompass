---
name: release
description: Bump this repo's version and push a vX.Y.Z tag, which builds and publishes the Linux packages
argument-hint: "[major|minor|patch]"
disable-model-invocation: true
model: sonnet
---

Cut a release of loginsicompass.

This file is also what `/release loginsicompass` follows when it is run from a
sicompass checkout, so `PROJECT_ROOT` is this repo's root.

**IMPORTANT: Prefix every command with `cd PROJECT_ROOT &&`.**

A release here is a **`vX.Y.Z` tag on `main`**. Pushing it runs
`.github/workflows/release.yml`, which builds the `.deb`, the `.rpm` and the
archive on Ubuntu 22.04 and publishes them as a GitHub release. desicompass's
flake pins this repo by that tag (its `loginsicompass` input). Nothing
in this repo is published to crates.io. Background:
[docs/releasing.md](../../../docs/releasing.md).

**This publishes to the world.** Everything before the tag push is checking.
Never move or delete a pushed tag. If a release is broken, release the next
patch.

**Order across repos.** `sicompass-ui` first, since this repo pins it, then this
one, then `desicompass`, which pins this one.

## Steps

1. **Clean state.** `git status --short` must be empty, and
   `git rev-list --left-right --count origin/main...main` must print `0  0`.
   If not, stop and say to run `/commit-and-push` first.

2. **Pins.** The release build checks out this repo alone.
   - Every `[patch...]` section in `Cargo.toml` is commented out.
   - `sicompass-ui` in `Cargo.toml` is `tag = `, never a bare `rev`, and the
     flake's `sicompass-ui` input is the same tag.
   If one has to move, run `/release sicompass-ui` first and move it through
   `/commit-and-push`, then start again.

3. **Checks, here.**
   ```sh
   xvfb-run -a cargo test
   cargo clippy --all-targets
   timeout 1200 nix build "git+file://$PWD"
   ```
   Also run any extra check `CLAUDE.md` lists under "Releasing".

4. **Checks, on GitHub.** Nothing runs on a commit, so nothing has been verified
   unless someone asked. The third run is the release itself with nothing
   published: it is the only thing that builds on Ubuntu rather than in the
   flake's toolchain and installs the result on a bare system, so it is what
   finds out that the packages still build and start.
   ```sh
   gh workflow run ci.yml && gh workflow run licenses.yml && gh workflow run release.yml
   gh run list --limit 3
   ```
   Wait for all three to go green before continuing. The Release run keeps the
   packages on the run as an artifact (`gh run download <id> -n packages`).

5. **Version.** Read `[package] version` in `Cargo.toml` and compare it with
   `git tag --sort=-v:refname | head -1`.
   - That version has no tag yet: it was bumped in an earlier commit, so release
     it as it is and skip step 6. Its `CHANGELOG.md` section must exist: the
     workflow fails without one, after the tag is public.
   - It matches the latest tag: bump it. Patch by default, `minor`/`major` if
     `$ARGUMENTS` says so.

6. **Bump.** Set the version in `Cargo.toml`, run
   `cargo update --workspace --offline`, and add a `CHANGELOG.md` section under
   the heading `## X.Y.Z`. That section becomes the body of the GitHub release,
   so it is what users read on the download page. Commit
   `Release: bump version to X.Y.Z` and `git push origin HEAD:main`.

7. **Tag.**
   ```sh
   git tag -a vX.Y.Z -m "Release vX.Y.Z"
   git push origin vX.Y.Z
   ```

8. **Follow the workflow.** `gh run list --workflow=release.yml --limit 1`. Judge
   success by the release, not the run status:
   ```sh
   gh release view vX.Y.Z --json assets --jq '.assets[].name'
   ```
   It holds exactly four assets: `loginsicompass_amd64.deb`,
   `loginsicompass.x86_64.rpm`,
   `loginsicompass-x86_64-unknown-linux-gnu.tar.xz` and `SHA256SUMS.txt`. Then
   these should return 200, since the README links them:
   ```sh
   curl -sIL -o /dev/null -w '%{http_code}\n' \
     https://github.com/friendlyflow/loginsicompass/releases/latest/download/loginsicompass_amd64.deb
   curl -sIL -o /dev/null -w '%{http_code}\n' \
     https://github.com/friendlyflow/loginsicompass/releases/latest/download/loginsicompass.x86_64.rpm
   ```

9. **Smoke test.** The list is in `docs/releasing.md`. A green run proves that a
   clean install starts nested in Xvfb, not that it takes over a real display. Say
   so if this was not done.

10. **Report** the tag and the release URL, and say that `desicompass` pins this
    repo: its flake's `loginsicompass` input has to move before it next releases.
