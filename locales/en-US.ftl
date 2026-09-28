# The login screen — English (source locale).
#
# Every key here must exist in the other three bundles too; a test in
# src/i18n.rs fails if one is missing. Keys start with `login-` because the
# bundles are appended to sicompass-ui's, and a clash is a registration error.

login-provider-name = Sign in

login-group-user = User
login-group-session = Session
login-label-password = Password

login-button-suspend = Suspend
login-button-reboot = Restart
login-button-poweroff = Shut down

login-announce-suspend = Suspending
login-announce-reboot = Restarting
login-announce-poweroff = Shutting down
login-power-failed = { $action } failed: { $error }

login-no-accounts = No accounts to sign in to
login-not-connected = Not connected to greetd
login-not-ready = Not ready for a password yet
login-no-session = No session to start
login-checking = Checking your password
login-wrong-password = Wrong password. Try again.
login-starting = Starting { $session }
login-announce-user = User { $name }
login-announce-session = Session { $name }

# Settings. The wording matches the app's own settings page.
login-setting-screen-reader = screen reader
login-setting-font-scale = font scale
login-setting-color-scheme = color scheme
login-setting-language = language
login-setting-shoulder-surfing = shoulder-surfing protection (blank screen)

login-color-dark = dark
login-color-light = light

# Each language in its own form, whatever the active locale, so a user can find
# theirs on a screen they cannot read.
login-language-en-US = English
login-language-nl-BE = Nederlands (België)
login-language-fr-BE = Français (Belgique)
login-language-de-BE = Deutsch (Belgien)

login-version = loginsicompass version
login-screen-reader-failed = Could not start the screen reader: { $error }
login-setting-changed = { $setting }: { $value }
login-on = on
login-off = off

# The clock.
login-clock = { $weekday } { $day } { $month } { $year }, { $time }
login-day-monday = Monday
login-day-tuesday = Tuesday
login-day-wednesday = Wednesday
login-day-thursday = Thursday
login-day-friday = Friday
login-day-saturday = Saturday
login-day-sunday = Sunday
login-month-1 = January
login-month-2 = February
login-month-3 = March
login-month-4 = April
login-month-5 = May
login-month-6 = June
login-month-7 = July
login-month-8 = August
login-month-9 = September
login-month-10 = October
login-month-11 = November
login-month-12 = December
