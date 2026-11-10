# The login screen — English (source locale).
#
# Every key here must exist in the other three bundles too; a test in
# src/i18n.rs fails if one is missing. Keys start with `login-` because the
# bundles are appended to sicompass-ui's, and a clash is a registration error.

login-provider-name = Sign in

login-group-user = User
login-group-session = Session
login-label-password = Password
login-show-password = show password

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

# The beginner's manual, opened with Tab: the login screen's own lines, read in
# this order. Written to sicompass-ui's docs/manual-guidelines.md: one idea per
# line, the key first, no em dashes and no semicolons, every line in all four
# bundles. MANUAL_LINES in src/gui.rs is the count.
login-manual-01 = Beginner's manual. Press the Down arrow key now to hear the next line, and the Up arrow key to go back one line. Escape leaves the manual, and Tab opens it again.
login-manual-02 = This is the login screen. It is one list, and each line is one element. Your screen reader reads the line you are on.
login-manual-03 = Home jumps to the first line of the list and End to the last. Up and Down move one line at a time.
login-manual-04 = Every line starts with a mark. An arrow right means there is more inside. A dash means there is nothing inside. The letter after it says what kind of element it is.
login-manual-05 = i is the password field. Press i to start typing your password, then Enter to sign in. Escape throws away what you typed.
login-manual-06 = A wrong password is said out loud and shown at the top. Press i and type it again.
login-manual-07 = R is a group of choices. Press Right to go into a group, move to a choice and press Enter. Left goes back out. The line of a group names the choice it holds.
login-manual-08 = The User group picks who signs in, and the Session group picks what starts after you sign in.
login-manual-09 = c is a checkbox. Press Enter to tick it or untick it. Show password shows what you type in the password field instead of hiding it.
login-manual-10 = b is a button. Press Enter on Suspend, Restart or Shut down, and this computer does it.
login-manual-11 = The line with the date and the time is the clock. It is text, and there is nothing to do on it.
login-manual-12 = The lines under the clock are settings. Screen reader turns the screen reader on or off. Shoulder-surfing protection blanks the screen, and the keys and the screen reader go on working.
login-manual-13 = Font scale makes the text bigger or smaller, color scheme picks dark or light, and language changes the language of this screen. They are groups of choices, and your choice is kept.
login-manual-14 = Search mode: press f and type to narrow the list down. Enter goes to the match and Escape cancels. Ctrl+F searches everything, the choices inside the groups too.
login-manual-15 = Press m to hear every key that works on the line you are on, and s to read the whole list as one page. Ctrl+W tells you where you are.
login-manual-16 = Beginner's manual mode is this list. Escape takes you back to the login screen, and Tab opens the manual again.
