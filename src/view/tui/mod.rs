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

pub mod widgets;

use synapse_rpc::message::SMessage;
use termion::{color, cursor};
use termion::event::Key;
use url::Url;

use std::io::Write;

use rpc::RpcContext;
use utils::align::{self, Align, Alignment};

// Unfortunately we cannot compose this inside View, so we need a composed trait
pub trait Component: Renderable + HandleInput + HandleRpc {}

pub trait Renderable {
    fn name(&self) -> String;
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16);
}

pub trait HandleInput {
    fn input(&mut self, rpc: &RpcContext, k: Key) -> InputResult;
}

pub trait HandleRpc {
    fn rpc(&mut self, rpc: &RpcContext, msg: &SMessage);
}

pub enum InputResult {
    Close,
    Rerender,
    ReplaceWith(Box<Component>),
    // FIXME: Special case for overlay, that introduces code duplication
    ReplaceWithNoDrop(Box<Component>),
    // A key was not used by any component below the current one
    Key(Key),
}

pub struct LoginPanel {
    server: widgets::Input,
    pass: widgets::PasswordInput,
    srv_selected: bool,
}

impl LoginPanel {
    pub fn new() -> LoginPanel {
        LoginPanel {
            server: widgets::Input::from("ws://:8412", 6),
            pass: widgets::PasswordInput::with_capacity(20),
            srv_selected: true,
        }
    }
}

impl Renderable for LoginPanel {
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, _: u16, _: u16) {
        let (srv, pass) = if self.srv_selected {
            (
                format!(
                    "{}Server{}: {}",
                    color::Fg(color::Cyan),
                    color::Fg(color::Reset),
                    self.server.format_active()
                ),
                format!("Pass: {}", self.pass.format_inactive()),
            )
        } else {
            (
                format!("Server: {}", self.server.format_inactive()),
                format!(
                    "{}Pass{}: {}",
                    color::Fg(color::Cyan),
                    color::Fg(color::Reset),
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
                match align::CenterLongestLeft::align_offset(lines, width) {
                    Alignment::Single(x) => x,
                    _ => unreachable!(),
                },
                height / 3
            )
        ).unwrap();
        align::Left::align(target, lines);
    }

    fn name(&self) -> String {
        "login".to_owned()
    }
}

impl HandleInput for LoginPanel {
    fn input(&mut self, ctx: &RpcContext, k: Key) -> InputResult {
        match k {
            Key::Down | Key::Up | Key::Char('\t') => {
                self.srv_selected = !self.srv_selected;
                InputResult::Rerender
            }
            Key::Left => {
                if self.srv_selected {
                    self.server.cursor_left();
                } else {
                    self.pass.cursor_left();
                };
                InputResult::Rerender
            }
            Key::Right => {
                if self.srv_selected {
                    self.server.cursor_right();
                } else {
                    self.pass.cursor_right();
                };
                InputResult::Rerender
            }
            Key::Backspace => {
                if self.srv_selected {
                    self.server.backspace();
                } else {
                    self.pass.backspace();
                }
                InputResult::Rerender
            }
            Key::Delete => {
                if self.srv_selected {
                    self.server.delete();
                } else {
                    self.pass.delete();
                }
                InputResult::Rerender
            }
            Key::Char('\n') => if let Err(err) = Url::parse(self.server.inner())
                .map_err(|err| format!("Server: {}", err))
                .and_then(|server| {
                    let pass = self.pass.inner();
                    ctx.init(server, pass)
                        .map_err(|err| format!("Synapse: {}", err))
                }) {
                let len = err.len();
                let overlay = Box::new(widgets::Overlay::new(
                    widgets::CloseOnInput::new(widgets::IgnoreRpc::new(
                        widgets::OwnedText::<align::Center>::new(err),
                    )),
                    Box::new(widgets::IgnoreRpcPassInput::new(*unsafe {
                        Box::from_raw(self as *const _ as *mut LoginPanel)
                    })),
                    (len as u16 + 2, 1),
                    color::Red,
                ));
                InputResult::ReplaceWithNoDrop(overlay as Box<Component>)
            } else {
                let panel = Box::new(widgets::Tabs::new(
                    vec![
                        Box::new(widgets::VSplit::new(
                            TrackerPanel::new(),
                            widgets::HSplit::new(
                                TorrentPanel::new(),
                                TorrentDetailsPanel::new(),
                                true,
                                0.65,
                            ),
                            false,
                            0.2,
                        )),
                        Box::new(StatisticsPanel::new()),
                    ],
                    0,
                ));
                InputResult::ReplaceWith(panel as Box<Component>)
            },
            Key::Char(c) => {
                if self.srv_selected {
                    self.server.push(c);
                } else {
                    self.pass.push(c);
                }
                InputResult::Rerender
            }
            _ => InputResult::Key(k),
        }
    }
}

pub struct TrackerPanel {}

impl TrackerPanel {
    pub fn new() -> TrackerPanel {
        TrackerPanel {}
    }
}

impl Component for TrackerPanel {}

impl Renderable for TrackerPanel {
    fn name(&self) -> String {
        "trackers".into()
    }
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        widgets::BorrowedText::<align::Center>::new("tracker panel").render(
            target,
            width,
            height,
            x_off,
            y_off + height / 2,
        );
    }
}

impl HandleInput for TrackerPanel {
    fn input(&mut self, ctx: &RpcContext, k: Key) -> InputResult {
        InputResult::Key(k)
    }
}

impl HandleRpc for TrackerPanel {
    fn rpc(&mut self, ctx: &RpcContext, msg: &SMessage) {}
}

pub struct TorrentPanel {}

impl TorrentPanel {
    pub fn new() -> TorrentPanel {
        TorrentPanel {}
    }
}

impl Component for TorrentPanel {}

impl Renderable for TorrentPanel {
    fn name(&self) -> String {
        "torrents".into()
    }
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        widgets::BorrowedText::<align::Center>::new("torrent panel").render(
            target,
            width,
            height,
            x_off,
            y_off + height / 2,
        );
    }
}

impl HandleInput for TorrentPanel {
    fn input(&mut self, ctx: &RpcContext, k: Key) -> InputResult {
        InputResult::Key(k)
    }
}

impl HandleRpc for TorrentPanel {
    fn rpc(&mut self, ctx: &RpcContext, msg: &SMessage) {}
}

pub struct StatisticsPanel {}

impl StatisticsPanel {
    pub fn new() -> StatisticsPanel {
        StatisticsPanel {}
    }
}

impl Component for StatisticsPanel {}

impl Renderable for StatisticsPanel {
    fn name(&self) -> String {
        "statistics".into()
    }
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        widgets::BorrowedText::<align::Center>::new("statistics panel").render(
            target,
            width,
            height,
            x_off,
            y_off + height / 2,
        );
    }
}

impl HandleInput for StatisticsPanel {
    fn input(&mut self, ctx: &RpcContext, k: Key) -> InputResult {
        InputResult::Key(k)
    }
}

impl HandleRpc for StatisticsPanel {
    fn rpc(&mut self, ctx: &RpcContext, msg: &SMessage) {}
}

pub struct TorrentDetailsPanel {}

impl TorrentDetailsPanel {
    pub fn new() -> TorrentDetailsPanel {
        TorrentDetailsPanel {}
    }
}

impl Component for TorrentDetailsPanel {}

impl Renderable for TorrentDetailsPanel {
    fn name(&self) -> String {
        "torrent details".into()
    }
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        widgets::BorrowedText::<align::Center>::new("torrent details panel").render(
            target,
            width,
            height,
            x_off,
            y_off + height / 2,
        );
    }
}

impl HandleInput for TorrentDetailsPanel {
    fn input(&mut self, ctx: &RpcContext, k: Key) -> InputResult {
        match k {
            Key::Char('q') => InputResult::Close,
            _ => InputResult::Key(k),
        }
    }
}

impl HandleRpc for TorrentDetailsPanel {
    fn rpc(&mut self, ctx: &RpcContext, msg: &SMessage) {}
}
