# Changelog

## 0.2.2

Built on sicompass-ui 0.2.2.

- The list looks like the app's: icons in the prefixes, a back arrow on the parent line, links and buttons that stand out, and a rounded border on the row you are on.
- Packages for Debian, Ubuntu, Mint, Fedora, RHEL and openSUSE: a `.deb`, an `.rpm` and an archive, built on Ubuntu 22.04 and installed in a bare system before they are published. They install the greeter and leave choosing it to you.
- The packages carry the Bootstrap Icons licence.

## 0.2.1

- The accessibility settings (screen reader, font size, colours, language) are
  shared with the session, both ways: a change made here is the session's too,
  and the other way round. They are offered on the login screen.
- The login screen speaks on first use. Orca starts in its own session, reads
  the focused row as soon as it listens, follows cursor moves, and its quitting
  by itself is logged.
- A wrong password is shown and said. The password row is a password field to
  the screen reader, and a show-password box sits under it.
- The last row is "loginsicompass version", and the window is named
  loginsicompass.
- Fixed a deadlock during login.
- The Nix build includes the locale bundles.
