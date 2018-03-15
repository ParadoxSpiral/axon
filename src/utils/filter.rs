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

use synapse_rpc::criterion::{Criterion, Operation, Value};
use synapse_rpc::message::CMessage;
use synapse_rpc::resource::ResourceKind;
use termion::color;

use rpc::RpcContext;
use view::tui::widgets;

#[derive(Clone)]
enum FilterMode {
    Insensitive,
    Sensitive,
}

impl FilterMode {
    fn cycle(&mut self) {
        match *self {
            FilterMode::Insensitive => {
                *self = FilterMode::Sensitive;
            }
            FilterMode::Sensitive => {
                *self = FilterMode::Insensitive;
            }
        }
    }
}

#[derive(Clone)]
pub struct Filter {
    mode: FilterMode,
    input: widgets::Input,
    serial: u64,
}

macro_rules! push_name {
    ($n: ident, $s: ident) => {
        if !$n.is_empty() {
            $n.push(' ');
        }
        $n.push_str($s);
    };
}

impl Filter {
    pub fn new(serial: u64) -> Filter {
        Filter {
            mode: FilterMode::Insensitive,
            input: widgets::Input::from("", 1),
            serial,
        }
    }

    pub fn init(&self, ctx: &RpcContext) {
        ctx.send(CMessage::FilterSubscribe {
            serial: self.serial,
            kind: ResourceKind::Torrent,
            criteria: Vec::new(),
        });
    }

    pub fn update(&self, ctx: &RpcContext) {
        let mut criteria_torrent = Vec::with_capacity(1);
        let mut name = String::new();

        for mut w in self.input.inner().split_whitespace() {
            let mut l = w.char_indices();
            // This guards against bigger than 1 byte code points, and a criterion not having been
            // fully written yet
            if l.next().map(|l| l.0).unwrap_or(1) != 0 || l.next().map(|l| l.0).unwrap_or(0) != 1
                || l.next().map(|l| l.0).unwrap_or(0) != 2
            {
                push_name!(name, w);
                continue;
            }

            match &w[..1] {
                "t" => if &w[1..2] == ":" {
                    criteria_torrent.push(Criterion {
                        field: "tracker_urls".into(),
                        op: Operation::Has,
                        value: Value::S(w[2..].to_owned()),
                    });
                },
                "p" => if let Ok(n) = w[2..].parse::<f32>() {
                    criteria_torrent.push(Criterion {
                        field: "progress".into(),
                        op: match &w[1..2] {
                            ":" => Operation::Eq,
                            "<" => Operation::LT,
                            ">" => Operation::GT,
                            _ => {
                                // TODO: Insert red BG
                                continue;
                            }
                        },
                        value: Value::F(n / 100.),
                    })
                } else {
                    // TODO: Insert red BG
                },
                "s" => if let Ok(n) = w[2..].parse::<f32>() {
                    criteria_torrent.push(Criterion {
                        field: "size".into(),
                        op: match &w[1..2] {
                            "<" => Operation::LTE,
                            ">" => Operation::GTE,
                            _ => {
                                // TODO: Insert red BG
                                continue;
                            }
                        },
                        value: Value::F(n * 1024. * 1024.),
                    });
                } else {
                    criteria_torrent.push(Criterion {
                        field: "status".into(),
                        op: Operation::Eq,
                        value: match &w[2..3] {
                            "i" => Value::S("idle".to_owned()),
                            "s" => Value::S("seeding".to_owned()),
                            "l" => Value::S("leeching".to_owned()),
                            "e" => Value::S("error".to_owned()),
                            "p" => Value::S("paused".to_owned()),
                            "n" => Value::S("pending".to_owned()),
                            "h" => Value::S("hashing".to_owned()),
                            "m" => Value::S("magnet".to_owned()),
                            _ => {
                                // TODO: Insert red BG
                                continue;
                            }
                        },
                    });
                },
                _ => {
                    push_name!(name, w);
                }
            }
        }

        if !name.is_empty() {
            criteria_torrent.push(Criterion {
                field: "name".into(),
                op: match self.mode {
                    FilterMode::Insensitive => Operation::ILike,
                    FilterMode::Sensitive => Operation::Like,
                },
                value: Value::S(name),
            });
        }

        ctx.send(CMessage::FilterSubscribe {
            serial: self.serial,
            kind: ResourceKind::Torrent,
            criteria: criteria_torrent,
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
