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

use synapse_rpc::message::SMessage;
use termion::{cursor, event::Key};

use std::io::Write;

use crate::{
    config::CONFIG,
    tui::{widgets, Component, HandleInput, HandleRpc, InputResult, Renderable},
    utils::{
        align::{self, x::Align},
        color::ColorEscape,
    },
};

#[derive(Clone)]
pub struct Login {
    server: widgets::Input,
    pass: widgets::PasswordInput,
    srv_selected: bool,
}

impl Login {
    pub fn new() -> Login {
        Login {
            server: CONFIG
                .server
                .as_ref()
                .map(|s| widgets::Input::from(s.clone(), s.len() + 1))
                .unwrap_or_else(|| widgets::Input::from("ws://:8412".into(), 6)),
            pass: CONFIG
                .pass
                .as_ref()
                .map(|s| widgets::PasswordInput::from(s.clone(), s.len() + 1))
                .unwrap_or_else(|| widgets::PasswordInput::with_capacity(20)),
            srv_selected: true,
        }
    }
}

impl Component for Login {}

impl HandleRpc for Login {
    fn rpc(&mut self, _: SMessage) -> bool {
        false
    }
}

impl Renderable for Login {
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, _: u16, _: u16) {
        let (srv, pass) = if self.srv_selected {
            (
                format!(
                    "{}Server{}: {}",
                    ColorEscape::cyan(),
                    ColorEscape::reset(),
                    self.server.format_active()
                ),
                format!("Pass: {}", self.pass.format_inactive()),
            )
        } else {
            (
                format!("Server: {}", self.server.format_inactive()),
                format!(
                    "{}Pass{}: {}",
                    ColorEscape::cyan(),
                    ColorEscape::reset(),
                    self.pass.format_active()
                ),
            )
        };
        let lines = &[
            "Welcome to axon, the synapse TUI",
            "Login to a synapse instance:",
            &srv,
            &pass,
        ];

        write!(
            target,
            "{}",
            cursor::Goto(
                match align::x::CenterLongestLeft::align_offset(lines, width) {
                    align::x::Alignment::Single(x) => x,
                    _ => unreachable!(),
                },
                height / 3
            )
        )
        .unwrap();
        align::x::Left::align(target, lines);
    }
}

impl HandleInput for Login {
    fn input(&mut self, k: Key, _: u16, _: u16) -> InputResult {
        match k {
            Key::Home => {
                if self.srv_selected {
                    self.server.home();
                } else {
                    self.pass.home();
                }
            }

            Key::End => {
                if self.srv_selected {
                    self.server.end();
                } else {
                    self.pass.end();
                }
            }

            Key::Down | Key::Up | Key::Char('\t') => {
                self.srv_selected = !self.srv_selected;
            }

            Key::Left => {
                if self.srv_selected {
                    self.server.cursor_left();
                } else {
                    self.pass.cursor_left();
                }
            }

            Key::Right => {
                if self.srv_selected {
                    self.server.cursor_right();
                } else {
                    self.pass.cursor_right();
                }
            }

            Key::Backspace => {
                if self.srv_selected {
                    self.server.backspace();
                } else {
                    self.pass.backspace();
                }
            }

            Key::Delete => {
                if self.srv_selected {
                    self.server.delete();
                } else {
                    self.pass.delete();
                }
            }

            Key::Char('\n') => {
                return InputResult::ConnectWith(
                    self.server.inner().to_string(),
                    self.pass.inner().to_string(),
                );
            }

            Key::Char(c) => {
                if self.srv_selected {
                    self.server.push(c);
                } else {
                    self.pass.push(c);
                }
            }
            _ => {
                return InputResult::Key(k);
            }
        }
        InputResult::Rerender
    }
}
