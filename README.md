# axon
The synapse TUI client


# Usage
Note: Currently termion does not respect terminfo and uses ANSI color codes ([#106](https://github.com/ticki/termion/issues/106)).

## Keybindings
- `^d` disconnects from the current server, and closes axon when in the login panel
- `H` switches panel focus left, `K` up, `L` right, `J` down. The lowercase variants may switch items
- `\n` confirms an action, or engages focus
- arrow keys switch items

Torrent panel:
- `^s` cycles filtering mode (case sensitive -> case insentive ->0)
- `^f` focuses the filter input
- `esc` clears the filter
- `d` opens the selected torrents' details
- `t` toggles displayal of the list of trackers

Torrent details:
- `q` closes the current details pane

# Windows
Termion currently does not support windows, but might in the future ([#103](https://github.com/ticki/termion/issues/103)) (I really don't care about Windows, and dislike you for using it).
