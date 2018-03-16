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

use chrono::{DateTime, Local, Utc};

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
    res + &*format!("{:02}:{:02}:{:02}", h, m, s)
}

pub trait FormatSize {
    fn fmt_size(self) -> String;
}

static SCALE: [&'static str; 9] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB", "ZiB", "YiB"];
macro_rules! impl_fmt_size {
    ($impl_ty: ty) => {
        impl FormatSize for $impl_ty {
            fn fmt_size(self) -> String {
                let mut size = self as f32;
                let mut idx = 0;
                while size >= 1024. {
                    size /= 1024.;
                    idx += 1;
                }

                format!("{:06.2} {}", size, SCALE[idx])
            }
        }
    };
}

impl_fmt_size!(usize);
impl_fmt_size!(u32);
impl_fmt_size!(u64);
impl_fmt_size!(i64);
