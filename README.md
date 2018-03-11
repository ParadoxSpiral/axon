# axon
The synapse TUI client


# Usage
Note: Currently termion does not respect terminfo and uses ANSI color codes ([#106](https://github.com/ticki/termion/issues/106)).

## Keybindings
- vim style movement, with uppercase letters switching panel focus
- `^q` disconnects from the current server, or closes axon when in the login panel
- `E` display errors of the currently selected torrent's trackers
- `e` display the error of the currently selected torrent

Torrent panel:
- `^f` opens the filter input
- `\n` focuses the filter input
- `d` opens the selected torrents' details
- `t` toggles displayal of the list of trackers
- `PgUp/Down` scrolls by one panel height

Filter input:
- `esc` defocuses
- `^f` closes
- `^s` cycles filtering mode (case sensitive, case insentive)

Torrent details:
- `q` closes the current details panel

# Windows
Termion currently does not support windows, but might in the future ([#103](https://github.com/ticki/termion/issues/103)).
