#!/usr/bin/env bash
#
# Print the GitHub release body for a version: the install table, then that
# version's section of CHANGELOG.md. Fails when the section is missing, because
# what would go out instead is an empty page.
#
# Usage: scripts/release-notes.sh 0.2.3

set -euo pipefail
cd "$(dirname "$0")/.."

version="$1"
tag="v$version"
base="https://github.com/friendlyflow/loginsicompass/releases/download/$tag"

changes="$(awk -v heading="## $version" '
    $0 == heading { found = 1; next }
    found && /^## / { exit }
    found { print }
' CHANGELOG.md)"
test -n "$(tr -d '[:space:]' <<<"$changes")" \
    || { echo "::error::CHANGELOG.md has no section for $version" >&2; exit 1; }

cat <<NOTES
## Install

| Platform | Download |
| --- | --- |
| Debian, Ubuntu | [loginsicompass_amd64.deb]($base/loginsicompass_amd64.deb) |
| Fedora, openSUSE | [loginsicompass.x86_64.rpm]($base/loginsicompass.x86_64.rpm) |
| Any Linux, binary only | [loginsicompass-x86_64-unknown-linux-gnu.tar.xz]($base/loginsicompass-x86_64-unknown-linux-gnu.tar.xz) |
| Nix, NixOS | \`github:friendlyflow/loginsicompass/$tag\`, see the [README](https://github.com/friendlyflow/loginsicompass#install-on-nixos) |

The packages need [desicompass](https://github.com/friendlyflow/desicompass/releases/latest) and greetd. The [README](https://github.com/friendlyflow/loginsicompass#install-on-debian-ubuntu-and-fedora) has the commands, and says what to do before switching login screens.

## What changed
$changes
NOTES
