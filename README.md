# axon
The synapse TUI client


# Usage
Note: Currently termion does not respect terminfo and uses e.g. ANSI color codes ([#106](https://github.com/ticki/termion/issues/106)).

## Hotkeys
- `^d` closes axon
- `h` switches pane focus left, `k` up, `l` right, `j` down
- arrow keys generally switch items

Torrent panel:
- `^i` will filter case insensitively, `^s` sensitively
- `^d` will open the selected torrents' details

Torrent details:
- `q` will close the current details pane

Tracker panel:
- same filterung rules as above


# Windows
Termion currently does not support windows, but might in the future ([#103](https://github.com/ticki/termion/issues/103)) (I really don't care about Windows, and dislike you for using it).
