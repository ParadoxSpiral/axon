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

use synapse_rpc::{
    criterion::{Criterion, Operation, Value},
    message::CMessage,
    resource::ResourceKind,
};
use termion::{color, event::Key};

use crate::{
    rpc,
    tui::{widgets, HandleInput, InputResult},
};

use std::sync::Arc;

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
    sink: rpc::WsSink,
}

impl Filter {
    pub fn new(sink: &rpc::WsSink) -> Filter {
        let serial = rpc::next_serial();
        rpc::send(
            sink,
            CMessage::FilterSubscribe {
                serial,
                kind: ResourceKind::Torrent,
                criteria: Vec::new(),
            },
        );

        Filter {
            mode: FilterMode::Insensitive,
            input: widgets::Input::from("".into(), 1),
            serial,
            sink: Arc::clone(sink),
        }
    }

    pub fn reset(&mut self) {
        self.input.clear();
        rpc::send(
            &self.sink,
            CMessage::FilterSubscribe {
                serial: self.serial,
                kind: ResourceKind::Torrent,
                criteria: Vec::new(),
            },
        );
    }

    fn update(&self) {
        let mut criteria = Vec::with_capacity(1);
        let mut name = String::new();

        for w in self.input.inner().split_whitespace() {
            let mut l = w.char_indices();
            // This guards against bigger than 1 byte code points, and a criterion not having been
            // fully written yet
            if l.next().map(|l| l.0).unwrap_or(1) != 0
                || l.next().map(|l| l.0).unwrap_or(0) != 1
                || l.next().map(|l| l.0).unwrap_or(0) != 2
            {
                if !name.is_empty() {
                    name.push(' ');
                }
                name.push_str(w);
                continue;
            }

            match &w[..1] {
                "t" => {
                    if &w[1..2] == ":" {
                        criteria.push(Criterion {
                            field: "tracker_urls".into(),
                            op: Operation::Has,
                            value: Value::S(w[2..].to_owned()),
                        });
                    }
                }
                "p" => {
                    if let Ok(n) = w[2..].parse::<f32>() {
                        criteria.push(Criterion {
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
                    }
                }
                "s" => {
                    if let Ok(n) = w[2..].parse::<f32>() {
                        criteria.push(Criterion {
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
                        criteria.push(Criterion {
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
                    }
                }
                _ => {
                    if !name.is_empty() {
                        name.push(' ');
                    }
                    name.push_str(w);
                }
            }
        }

        if !name.is_empty() {
            criteria.push(Criterion {
                field: "name".into(),
                op: match self.mode {
                    FilterMode::Insensitive => Operation::ILike,
                    FilterMode::Sensitive => Operation::Like,
                },
                value: Value::S(name),
            });
        }

        rpc::send(
            &self.sink,
            CMessage::FilterSubscribe {
                serial: self.serial,
                kind: ResourceKind::Torrent,
                criteria,
            },
        );
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

impl HandleInput for Filter {
    fn input(&mut self, k: Key, _: u16, _: u16) -> InputResult {
        match k {
            Key::Ctrl('s') => {
                self.mode.cycle();
                self.update();
            }
            Key::Backspace => {
                self.input.backspace();
                self.update();
            }
            Key::Delete => {
                self.input.delete();
                self.update();
            }
            Key::Home => {
                self.input.home();
            }
            Key::End => {
                self.input.end();
            }
            Key::Left => self.input.cursor_left(),
            Key::Right => self.input.cursor_right(),
            Key::Char(c) => {
                self.input.push(c);
                self.update();
            }
            _ => {
                return InputResult::Key(k);
            }
        }
        InputResult::Rerender
    }
}
