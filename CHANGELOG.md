# Changelog

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
