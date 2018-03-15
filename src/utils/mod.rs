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

use chrono::{DateTime, Local, Utc};
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

pub fn date_diff_now(date: DateTime<Utc>) -> String {
    let dur = Local::now().signed_duration_since(date);
    let w = dur.num_weeks();
    let d = dur.num_days() - dur.num_weeks() * 7;
    let h = dur.num_hours() - dur.num_days() * 24;
    let m = dur.num_minutes() - dur.num_hours() * 60;
    let s = dur.num_seconds() - dur.num_minutes() * 60;
    let mut res = String::with_capacity(
        8 + if w == 0 {
            0
        } else {
            2 + (w as f32).log10().trunc() as usize
        } + if d == 0 { 0 } else { 2 },
    );
    if w > 0 {
        if d > 0 {
            res += &*format!("{}w", w);
        } else {
            res += &*format!("{}w ", w);
        }
    }
    if d > 0 {
        res += &*format!("{}d ", d);
    }
    res + &*format!("{:0>2}:{:0>2}:{:0>2}", h, m, s)
}
