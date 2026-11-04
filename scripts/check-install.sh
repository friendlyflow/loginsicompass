#!/usr/bin/env bash
#
# Install the .deb on a bare Ubuntu with nothing but what it declares, and start
# the login screen the way its session script does. release.yml runs this in a
# container, and it has to stay one: a runner, or anyone's desktop, already has
# the libraries that are the question. SDL3 dlopens libwayland-client, so nothing
# but this notices a package that does not declare it.
#
# Usage, as root: scripts/check-install.sh dist/loginsicompass_amd64.deb

set -euo pipefail

deb="$(realpath "$1")"
export DEBIAN_FRONTEND=noninteractive

apt-get update -qq
apt-get install -y -qq --no-install-recommends curl ca-certificates >/dev/null

# The compositor it runs in, and through it the app. desicompass is released after
# this repo, so this is its last release, and before its first release with
# packages there is nothing to install beside this one.
work="$(mktemp -d)"
desicompass="$work/desicompass_amd64.deb"
if ! curl -fsSL -o "$desicompass" \
    https://github.com/friendlyflow/desicompass/releases/latest/download/desicompass_amd64.deb; then
    echo "::notice::desicompass has no released package yet, so the login screen was not started"
    exit 0
fi
latest="$(curl -fsSLI -o /dev/null -w '%{url_effective}' https://github.com/friendlyflow/sicompass/releases/latest)"
tag="${latest##*/}"
sicompass="$work/sicompass_${tag#v}_amd64.deb"
curl -fsSL -o "$sicompass" \
    "https://github.com/friendlyflow/sicompass/releases/download/$tag/$(basename "$sicompass")"

# --no-install-recommends, so what is declared is all there is.
apt-get install -y -qq --no-install-recommends "$sicompass" "$desicompass" "$deb" >/dev/null
dpkg -l loginsicompass greetd | tail -2

test "$(stat -c '%U' /var/lib/loginsicompass)" = _greetd \
    || { echo "::error::/var/lib/loginsicompass does not belong to the greetd account"; exit 1; }
id -nG _greetd | grep -qw sicompass-a11y \
    || { echo "::error::the greetd account is not in the sicompass-a11y group"; exit 1; }
grep -q loginsicompass /etc/greetd/config.toml \
    && { echo "::error::the package changed greetd's configuration"; exit 1; }
sh -n /usr/bin/loginsicompass-session

# There is no display to take over here, so it runs nested in Xvfb, with the
# session script's own command but for the backend and the journal. Only the
# nested X11 backend loads these libraries, and a real session never does, which
# is why the packages do not declare them.
apt-get install -y -qq --no-install-recommends \
    xvfb xauth libxkbcommon-x11-0 libxcursor1 libxi6 libxrandr2 libx11-xcb1 >/dev/null
script="$work/session"
sed -e 's/exec systemd-cat --identifier=loginsicompass dbus-run-session/exec dbus-run-session/' \
    -e 's/--backend tty/--backend auto/' /usr/bin/loginsicompass-session > "$script"
chmod 755 "$script"

export XDG_RUNTIME_DIR=/tmp/check-install
mkdir -p "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
log="$(mktemp)"
RUST_LOG=info timeout -k 5 20 xvfb-run -a "$script" >"$log" 2>&1 || true
cat "$log"

grep -q 'starting the gpu greeter' "$log" \
    || { echo "::error::a clean install never started the greeter"; exit 1; }
grep -q 'offering .* session(s)' "$log" \
    || { echo "::error::a clean install never reached the login page"; exit 1; }
if grep -q 'starting the shm greeter' "$log"; then
    echo "::error::the GPU path died on a clean install"
    exit 1
fi
echo "A clean install starts the login screen."
