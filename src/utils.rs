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

pub mod align {
    pub mod x {
        use termion::cursor;
        use std::io::Write;
        use super::super::*;
        pub trait Align {
            fn align_offset(lines: &[&str], width: u16) -> Alignment;
            fn align(_target: &mut Vec<u8>, _lines: &[&str]) {
                unimplemented!()
            }
        }

        pub enum Alignment {
            Single(u16),
            Each(Vec<u16>),
        }

        pub struct Left {}
        impl Align for Left {
            fn align(target: &mut Vec<u8>, lines: &[&str]) {
                for l in lines {
                    write!(
                        target,
                        "{}{}{}",
                        l,
                        cursor::Left(count_without_styling(l) as u16),
                        cursor::Down(1)
                    ).unwrap();
                }
            }
            fn align_offset(_: &[&str], _: u16) -> Alignment {
                Alignment::Single(0)
            }
        }

        pub struct Center {}
        impl Align for Center {
            fn align_offset(lines: &[&str], width: u16) -> Alignment {
                let mut algns = Vec::with_capacity(lines.len());
                for l in lines {
                    algns.push((width / 2).saturating_sub((l.len() / 2) as u16));
                }
                Alignment::Each(algns)
            }
        }

        pub struct CenterLongestLeft {}
        impl Align for CenterLongestLeft {
            fn align_offset(lines: &[&str], width: u16) -> Alignment {
                let max_len = lines
                    .iter()
                    .map(|l| count_without_styling(*l))
                    .max()
                    .unwrap() as u16;

                Alignment::Single((width / 2).saturating_sub(max_len / 2))
            }
        }
    }

    pub mod y {
        pub trait Align {
            // Unlike the x alignment, this may not vary
            fn align_offset(lines: &[&str], height: u16) -> u16;
        }

        pub struct Top {}
        impl Align for Top {
            fn align_offset(_: &[&str], _: u16) -> u16 {
                0
            }
        }

        pub struct Center {}
        impl Align for Center {
            fn align_offset(lines: &[&str], height: u16) -> u16 {
                (height / 2).saturating_sub(lines.len() as u16)
            }
        }
    }
}

use chrono::{DateTime, Utc};
use synapse_rpc::criterion::{Criterion, Operation, Value};
use synapse_rpc::message::CMessage;
use synapse_rpc::resource::ResourceKind;
use termion::color;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use rpc::RpcContext;
use view::tui::widgets;

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

pub fn count(l: &str) -> usize {
    l.graphemes(true).map(|g| g.width()).sum()
}

pub fn date_diff_now(date: DateTime<Utc>) -> String {
    let dur = Utc::now().signed_duration_since(date);
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

#[derive(Clone)]
enum FilterMode {
    Insensitive,
    Sensitive,
}

impl FilterMode {
    fn cycle(&mut self) {
        match self {
            &mut FilterMode::Insensitive => {
                *self = FilterMode::Sensitive;
            }
            &mut FilterMode::Sensitive => {
                *self = FilterMode::Insensitive;
            }
        }
    }
}

pub struct Filter {
    mode: FilterMode,
    input: widgets::Input,
    s1: u64,
    s2: u64,
}

impl Filter {
    pub fn new(serial_torr: u64, serial_trac: u64) -> Filter {
        Filter {
            mode: FilterMode::Insensitive,
            input: widgets::Input::from("", 1),
            s1: serial_torr,
            s2: serial_trac,
        }
    }

    pub fn init(&self, ctx: &RpcContext) {
        ctx.send(CMessage::FilterSubscribe {
            serial: self.s1,
            kind: ResourceKind::Torrent,
            criteria: Vec::new(),
        });
        ctx.send(CMessage::FilterSubscribe {
            serial: self.s2,
            kind: ResourceKind::Tracker,
            criteria: Vec::new(),
        });
    }

    pub fn update(&self, ctx: &RpcContext) {
        // TODO: Actual filtering syntax
        ctx.send(CMessage::FilterSubscribe {
            serial: self.s1,
            kind: ResourceKind::Torrent,
            criteria: vec![
                Criterion {
                    field: "name".into(),
                    op: match self.mode {
                        FilterMode::Insensitive => Operation::ILike,
                        FilterMode::Sensitive => Operation::Like,
                    },
                    value: Value::S(self.input.inner().into()),
                },
            ],
        });
    }

    pub fn cycle(&mut self) {
        self.mode.cycle();
    }

    pub fn format(&self, active: bool) -> String {
        let (c_s, c_e, cnt) = if active {
            (
                format!("{}", color::Fg(color::Cyan)),
                format!("{}", color::Fg(color::Reset)),
                self.input.format_active(),
            )
        } else {
            ("".into(), "".into(), self.input.format_inactive().into())
        };
        format!(
            "{}{}{}{}",
            c_s,
            match self.mode {
                FilterMode::Insensitive => "Filter[i]: ",
                FilterMode::Sensitive => "Filter[s]: ",
            },
            c_e,
            cnt
        )
    }
}

impl ::std::ops::Deref for Filter {
    type Target = widgets::Input;

    fn deref(&self) -> &Self::Target {
        &self.input
    }
}

impl ::std::ops::DerefMut for Filter {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.input
    }
}
