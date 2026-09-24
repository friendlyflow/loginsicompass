# loginsicompass

*An accessible login screen for Sicompass.*

loginsicompass is part of [Sicompass](https://github.com/friendlyflow/sicompass),
a keyboard-first, accessibility-first way to use your entire computer. It is the
screen you log in on, a greeter for [greetd](https://git.sr.ht/~kennylevinsen/greetd).

It draws the login screen with the same renderer as Sicompass itself, so a
screen-reader user meets the same list, the same spoken prefixes and the same
masked password field at the login prompt as inside the app. The whole screen
is one list:

```
User          your account, chosen from a list
Session       Desicompass, or any other desktop you have installed
Password      the cursor starts here, Enter logs you in
Suspend
Restart
Shut down
the date and time
```

There is no login button. Enter in the password field logs you in, the same way
Enter confirms any other field in Sicompass. The user and session you last
logged in with are remembered for the next time.

If the graphics driver fails, the login screen restarts itself once in a plain
software mode, so there is always a way in. That mode has no text and no screen
reader support. It exists only so you can get in and fix things.

## Install on NixOS

loginsicompass is set up by the NixOS module in
[desicompass](https://github.com/friendlyflow/desicompass), the Sicompass
session:

```nix
services.desicompass.enable = true;           # first, and check that it works
services.desicompass.greeter.enable = true;   # then replace the login screen
```

Turn the greeter on only once the session itself works. A login screen that
fails to start leaves no graphical way in, so recovery is a text console and
`nixos-rebuild --rollback`. Its output goes to the journal:
`journalctl -t loginsicompass -b`.

## Trying it without logging out

A fake greetd that accepts one password, and the greeter nested inside
desicompass in your current session:

```bash
nix develop
cargo run --example fake-greetd -- /tmp/greetd.sock hunter2

# in a second terminal, with desicompass checked out next to this repo
cargo build
GREETD_SOCK=/tmp/greetd.sock cargo run --manifest-path ../desicompass/Cargo.toml -- \
  --backend auto \
  --startup-cmd "$PWD/target/debug/loginsicompass --state-dir /tmp/lsc-state"
```

[docs/greeter.md](docs/greeter.md) explains the rest: the greetd conversation,
how users and sessions are found, what happens to the password, and the
fallback.

## Building from source

```bash
nix develop          # optional, brings the whole toolchain
cargo build --release
xvfb-run -a cargo test
```

`nix build` builds the packaged version. loginsicompass runs on Linux only.

## Related repositories

- [desicompass](https://github.com/friendlyflow/desicompass), the session and
  the NixOS module that installs this greeter
- [sicompass-ui](https://github.com/friendlyflow/sicompass-ui), the renderer it
  shares with the app
- [sicompass](https://github.com/friendlyflow/sicompass), the application

## Community

Join the conversation on
[Discord](https://discord.com/channels/1464152138753249313/1464152139231137894).

## License

#### Open source license

If you are creating an open source application under a license compatible with
the GNU GPL license v3, you may use this project under the terms of the GPLv3.
See [LICENSE](LICENSE).

## Contributing

Contributions are welcome. Whether it is code, documentation, or feedback, your
input helps make computing more accessible for everyone.
