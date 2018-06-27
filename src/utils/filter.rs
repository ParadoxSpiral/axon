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
use termion::event::Key;

use rpc::RpcContext;
use tui::{widgets, HandleInput, InputResult};

static mut SERIAL: u64 = 0;

// This shall only be called once per server lifetime at initialization
pub unsafe fn init(ctx: &RpcContext) {
    SERIAL = ctx.next_serial();

    ctx.send(CMessage::FilterSubscribe {
        serial: SERIAL,
        kind: ResourceKind::Torrent,
        criteria: Vec::new(),
    });
}

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
}

impl HandleInput for Filter {
    fn input(&mut self, ctx: &RpcContext, k: Key, _: u16, _: u16) -> InputResult {
        match k {
            Key::Ctrl('s') => {
                self.mode.cycle();
                self.update(ctx);
            }
            Key::Backspace => {
                self.input.backspace();
                self.update(ctx);
            }
            Key::Delete => {
                self.input.delete();
                self.update(ctx);
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
                self.update(ctx);
            }
            _ => {
                return InputResult::Key(k);
            }
        }
        InputResult::Rerender
    }
}

macro_rules! push_name {
    ($n:ident, $s:ident) => {
        if !$n.is_empty() {
            $n.push(' ');
        }
        $n.push_str($s);
    };
}

impl Filter {
    pub fn new() -> Filter {
        Filter {
            mode: FilterMode::Insensitive,
            input: widgets::Input::from("".into(), 1),
        }
    }

    pub fn reset(&self, ctx: &RpcContext) {
        ctx.send(CMessage::FilterSubscribe {
            serial: unsafe { SERIAL },
            kind: ResourceKind::Torrent,
            criteria: Vec::new(),
        });
    }

    fn update(&self, ctx: &RpcContext) {
        let mut criteria = Vec::with_capacity(1);
        let mut name = String::new();

        for mut w in self.input.inner().split_whitespace() {
            let mut l = w.char_indices();
            // This guards against bigger than 1 byte code points, and a criterion not having been
            // fully written yet
            if l.next().map(|l| l.0).unwrap_or(1) != 0
                || l.next().map(|l| l.0).unwrap_or(0) != 1
                || l.next().map(|l| l.0).unwrap_or(0) != 2
            {
                push_name!(name, w);
                continue;
            }

            match &w[..1] {
                "t" => if &w[1..2] == ":" {
                    criteria.push(Criterion {
                        field: "tracker_urls".into(),
                        op: Operation::Has,
                        value: Value::S(w[2..].to_owned()),
                    });
                },
                "p" => if let Ok(n) = w[2..].parse::<f32>() {
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
                },
                "s" => if let Ok(n) = w[2..].parse::<f32>() {
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
                },
                _ => {
                    push_name!(name, w);
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

        ctx.send(CMessage::FilterSubscribe {
            serial: unsafe { SERIAL },
            kind: ResourceKind::Torrent,
            criteria,
        });
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
