#!/usr/bin/env bash
#
# Build the release's packages into dist/:
#
#   loginsicompass_amd64.deb                          Debian, Ubuntu
#   loginsicompass.x86_64.rpm                         Fedora, openSUSE
#   loginsicompass-x86_64-unknown-linux-gnu.tar.xz    the binary, unpack by hand
#   SHA256SUMS.txt
#
# release.yml runs this on ubuntu-22.04, after installing what its "Install
# build dependencies" step lists and running scripts/setup-freetype.sh. See
# docs/releasing.md.
#
# File names carry no version, so the README's releases/latest/download/ links
# never go stale. The version is inside each package.

set -euo pipefail
cd "$(dirname "$0")/.."

version="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)"
test -n "$version" || { echo "::error::no version in Cargo.toml"; exit 1; }

# A release built from a tag has to be the version it says it is.
if [ "${GITHUB_REF_TYPE:-}" = tag ] && [ "${GITHUB_REF_NAME:-}" != "v$version" ]; then
    echo "::error::tag ${GITHUB_REF_NAME} does not match version $version in Cargo.toml"
    exit 1
fi

# bundled-sdl3 links SDL3 statically, so an SDL3 upgrade elsewhere on the system
# cannot stop the login screen from starting, and most distributions do not
# package SDL3 yet anyway.
cargo build --release --locked --features bundled-sdl3

deps="$(ldd target/release/loginsicompass)"
echo "$deps"
if grep -q 'not found' <<<"$deps"; then
    echo "::error::loginsicompass needs a library the runner does not have"
    exit 1
fi
if grep -q 'libSDL3\|libfreetype' <<<"$deps"; then
    echo "::error::loginsicompass links SDL3 or FreeType dynamically, which a user's machine may not have"
    exit 1
fi
# A vendored FreeType has no PNG support and needs no libpng, so this is what
# catches colour emoji silently disappearing.
if ! grep -q libpng <<<"$deps"; then
    echo "::error::loginsicompass was built against a FreeType without PNG support"
    exit 1
fi
target/release/loginsicompass --help >/dev/null

rm -rf dist target/debian target/generate-rpm
mkdir -p dist

# The two package lists are in Cargo.toml, and tests/packaging.rs keeps them
# the same.
cargo deb --no-build --output dist/loginsicompass_amd64.deb
cargo generate-rpm --auto-req packaging/rpm-find-requires -o dist/loginsicompass.x86_64.rpm

listing="$(dpkg-deb -c dist/loginsicompass_amd64.deb)"
for file in usr/bin/loginsicompass usr/bin/loginsicompass-session; do
    grep -q "\./$file\$" <<<"$listing" \
        || { echo "::error::the .deb does not ship $file"; exit 1; }
done
dpkg-deb -f dist/loginsicompass_amd64.deb Package Version Depends Recommends

archive=loginsicompass-x86_64-unknown-linux-gnu
stage="$(mktemp -d)/$archive"
mkdir -p "$stage"
strip -o "$stage/loginsicompass" target/release/loginsicompass
cp packaging/loginsicompass-session LICENSE README.md CHANGELOG.md THIRD-PARTY-LICENSES.html "$stage/"
cp fonts/LICENSE-*.txt "$stage/"
tar -C "$(dirname "$stage")" -cJf "dist/$archive.tar.xz" "$archive"

(cd dist && sha256sum -- * > SHA256SUMS.txt && ls -la && cat SHA256SUMS.txt)
