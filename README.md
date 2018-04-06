# axon
The synapse TUI client

# Usage
Note: Currently termion (the underlying TUI library) does not respect terminfo and uses ANSI color codes ([#106](https://github.com/ticki/termion/issues/106)).

## Compilation dependencies
Rust minimum version of 1.24, pkg-config, gcc or clang, openssl.

## Keybindings
- `hjkl` movement, `HJKL` switch focus
- `C-q` disconnects from the current server, or closes axon when in the login panel
- `e` display errors of the currently selected torrent

Torrent panel:
- `C-f` opens the filter input
- `\n` focuses the filter input
- `d` opens the selected torrents' details
- `t` toggles displayal of the list of trackers
- `PgUp/Down` scrolls by one panel height

Filter input:
- `esc` defocuses
- `C-f` closes
- `C-s` cycles filtering mode (case sensitive, case insentive)

Filter specifiers:
Every word starting with a specifier `[name][sign][content]` refines the criteria, take care not to accidentally include them in the free text! Any other word refines the torrent name criteria in the order of occurence.
- `t:<%s>` tracker host name
- `s[<>]<%f>` torrent size in MB
- `s:[i s l e p pe h m]` torrent status (idle, seeding, leeching, error, paused, pending, hashing, magnet)
- `p[:<>]<%f>` torrent completion percent (0-100)

Torrent details:
- `q` closes the current details panel

# Windows
Termion currently does not support windows, but might in the future ([#103](https://github.com/ticki/termion/issues/103)).
