# axon
The synapse TUI client


# Usage
Note: Currently termion does not respect terminfo and uses ANSI color codes ([#106](https://github.com/ticki/termion/issues/106)).

## Keybindings
- `^d` disconnects from the current server, and closes axon when in the login panel
- `h` may switch panel focus left, `k` up, `l` right, `j` down
- `\n` may confirm an action, or engage focus
- `esc` may disengage focus
- arrow keys switch items, `\t` may switch items

Torrent panel:
- `^s` circles filtering case (in)sensitivity
- `^f` focuses the filter input
- `esc` clears the filter
- `d` opens the selected torrents' details
- `t` opens the tracker filter panel, trackers are always filtered for insensitively

Torrent details:
- `q` closes the current details pane

# Windows
Termion currently does not support windows, but might in the future ([#103](https://github.com/ticki/termion/issues/103)) (I really don't care about Windows, and dislike you for using it).
