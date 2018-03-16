// Copyright (C) 2017  ParadoxSpiral
//
// This file is part of Axon.
//
// Axon is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Axon is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Axon.  If not, see <http://www.gnu.org/licenses/>.

pub mod align;
pub mod filter;
pub mod fmt;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

// FIXME: Wide/Half chars, unicode-width only works for CJK iirc
pub fn count(l: &str) -> usize {
    l.graphemes(true).map(|g| g.width()).sum()
}

// FIXME: Wide/Half chars, unicode-width only works for CJK iirc
pub fn count_without_styling(l: &str) -> u16 {
    let mut count = 0;
    let mut gs = l.graphemes(true).map(|g| (g, g.width()));

    while let Some((g, w)) = gs.next() {
        // FIXME: This is only valid as long as termion doesn't use terminfo,
        // see: https://github.com/ticki/termion/issues/106
        if g == "\x1B" {
            // Skip to end of control sequence
            gs.position(|g| g.0 == "m").unwrap();
            continue;
        } else {
            count += w;
        }
    }

    count as u16
}
