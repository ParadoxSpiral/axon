# axon
The synapse TUI client

# Usage
Note: Currently termion (the underlying TUI library) does not respect terminfo and uses ANSI color codes ([#106](https://github.com/ticki/termion/issues/106)).

## Keybindings
- `e` display errors of the currently selected torrent
- `hjkl` movement, `HJKL` switch focus
- `C-q` disconnects from the current server, or closes axon when in the login panel

Torrent panel:
- `<PgUp>/<PgDown>` scrolls by one panel height
- `<ENTER>` opens selected torrent's directory
- `d` opens the selected torrent's details
- `f` opens/focuses the filter input
- `l` opens the rate limit panel
- `t` toggles displayal of the list of trackers

Filter input:
- `<ESC>` defocuses
- `C-f` removes the filter
- `C-s` cycles filtering mode (case sensitive, case insentive)

Filter specifiers:
Every word starting with a specifier `[name][sign][content]` refines the criteria, take care not to accidentally include them in the free text! Any other word refines the torrent name criteria in the order of occurence.
- `t:<%s>` tracker host name
- `s[<>]<%f>` torrent size in MB
- `s:[i s l e p pe h m]` torrent status (idle, seeding, leeching, error, paused, pending, hashing, magnet)
- `p[:<>]<%f>` torrent completion percent (0-100)

Torrent details:
- `q` closes the current details panel

Limits:
- `<ENTER>` Commit limits and close panel
- `<ESC>` Forget limits and close panel

## Configuration
The config file is searched for at `$XDG_CONFIG_HOME/axon/conf.toml` and `~/.config/axon/conf.toml`.
For options, see `example_conf.toml`.

## Compilation dependencies
Rust minimum version of 1.31, pkg-config, a cc, openssl/security-framework/schannel.


# Windows
Termion currently does not support windows, but might in the future ([#103](https://github.com/ticki/termion/issues/103)).
