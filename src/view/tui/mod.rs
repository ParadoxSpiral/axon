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

use synapse_rpc::criterion::{Criterion, Operation, Value};
use synapse_rpc::message::{CMessage, SMessage};
use synapse_rpc::resource::{Resource, ResourceKind, SResourceUpdate, Torrent, Tracker};
use termion::{color, cursor};
use termion::event::Key;
use url::Url;

use std::io::Write;

use rpc::RpcContext;
use utils::align;
use utils::align::x::Align;

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
    // A key was not used by any component below the current one
    Key(Key),
}

#[derive(Clone)]
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
                match align::x::CenterLongestLeft::align_offset(lines, width) {
                    align::x::Alignment::Single(x) => x,
                    _ => unreachable!(),
                },
                height / 3
            )
        ).unwrap();
        align::x::Left::align(target, lines);
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
                let overlay = Box::new(widgets::OwnedOverlay::new(
                    widgets::CloseOnInput::new(widgets::IgnoreRpc::new(
                        widgets::Text::<_, align::x::Center, align::y::Top>::new(err),
                    )),
                    Box::new(widgets::IgnoreRpcPassInput::new(self.clone())),
                    (len as u16 + 2, 1),
                    color::Red,
                ));
                InputResult::ReplaceWith(overlay as Box<Component>)
            } else {
                let panel = Box::new(widgets::Tabs::new(
                    vec![
                        Box::new(TorrentPanel::new()),
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum RFocus {
    Torrents,
    TorrentsFilter,
    Details,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum TFocus {
    Trackers,
    TrackersFilter,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Case {
    Sensitive,
    Insensitive,
}

struct TorrentPanel {
    rfocus: RFocus,
    r_act: bool,
    tfocus: Option<TFocus>,
    torrents: Vec<Torrent>,
    torrents_filter: (Case, widgets::Input),
    torrent_selected: usize,
    trackers: Vec<Tracker>,
    trackers_filter: (Case, widgets::Input),
    details: widgets::Tabs,
}

impl TorrentPanel {
    fn new() -> TorrentPanel {
        TorrentPanel {
            rfocus: RFocus::Torrents,
            r_act: true,
            tfocus: None,
            torrents: Vec::new(),
            torrents_filter: (Case::Insensitive, widgets::Input::from("", 1)),
            torrent_selected: 0,
            trackers: Vec::new(),
            trackers_filter: (Case::Insensitive, widgets::Input::from("", 1)),
            details: widgets::Tabs::new(Vec::new(), 0),
        }
    }
}

impl Component for TorrentPanel {}

impl HandleInput for TorrentPanel {
    fn input(&mut self, ctx: &RpcContext, k: Key) -> InputResult {
        match k {
            Key::Char('t') => {
                match (self.rfocus, self.r_act, self.tfocus) {
                    (RFocus::TorrentsFilter, true, _) => {
                        self.torrents_filter.1.push('t');
                    }
                    (_, false, Some(TFocus::TrackersFilter)) => {
                        self.trackers_filter.1.push('t');
                    }
                    (_, _, Some(_)) => {
                        self.tfocus = None;
                        self.r_act = true;
                    }
                    (_, _, None) => {
                        self.tfocus = Some(TFocus::TrackersFilter);
                        self.r_act = false;
                    }
                }
                InputResult::Rerender
            }
            Key::Char('d') => match (self.rfocus, self.r_act, self.tfocus) {
                (RFocus::Torrents, true, _) => if !self.torrents.is_empty() {
                    self.details.push(Box::new(TorrentDetailsPanel::new(
                        self.torrents[self.torrent_selected].clone(),
                    )));
                    InputResult::Rerender
                } else {
                    InputResult::Key(Key::Char('d'))
                },
                (RFocus::TorrentsFilter, true, _) => {
                    self.torrents_filter.1.push('d');
                    InputResult::Rerender
                }
                (_, false, Some(TFocus::TrackersFilter)) => {
                    self.trackers_filter.1.push('d');
                    InputResult::Rerender
                }
                (RFocus::Details, true, _) => self.details.input(ctx, k),
                (_, _, _) => InputResult::Key(Key::Char('d')),
            },
            Key::Esc => match (self.rfocus, self.r_act, self.tfocus) {
                (RFocus::TorrentsFilter, true, _) => {
                    self.rfocus = RFocus::Torrents;
                    InputResult::Rerender
                }
                (_, false, Some(TFocus::TrackersFilter)) => {
                    self.tfocus = Some(TFocus::Trackers);
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Esc),
            },
            Key::Char('\n') => match (self.rfocus, self.r_act, self.tfocus) {
                (RFocus::Torrents, true, _) => {
                    self.rfocus = RFocus::TorrentsFilter;
                    InputResult::Rerender
                }
                (_, false, Some(TFocus::Trackers)) => {
                    self.tfocus = Some(TFocus::TrackersFilter);
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Char('\n')),
            },
            Key::Char('h') => match (self.rfocus, self.r_act, self.tfocus) {
                (RFocus::TorrentsFilter, true, _) => {
                    self.torrents_filter.1.push('h');
                    InputResult::Rerender
                }
                (_, true, Some(_)) => {
                    self.r_act = false;
                    InputResult::Rerender
                }
                (_, false, Some(TFocus::TrackersFilter)) => {
                    self.trackers_filter.1.push('h');
                    InputResult::Rerender
                }
                (_, _, None) | (_, false, _) => InputResult::Key(Key::Char('h')),
            },
            Key::Char('l') => match (self.rfocus, self.r_act, self.tfocus) {
                (_, false, Some(TFocus::TrackersFilter)) => {
                    self.trackers_filter.1.push('l');
                    InputResult::Rerender
                }
                (_, false, _) => {
                    self.r_act = true;
                    InputResult::Rerender
                }
                (RFocus::TorrentsFilter, true, _) => {
                    self.torrents_filter.1.push('l');
                    InputResult::Rerender
                }
                (_, true, _) => InputResult::Key(Key::Char('l')),
            },
            Key::Char('j') => match (self.rfocus, self.r_act, self.tfocus) {
                (_, false, Some(TFocus::TrackersFilter)) => {
                    self.trackers_filter.1.push('j');
                    InputResult::Rerender
                }
                (RFocus::TorrentsFilter, true, _) => {
                    self.torrents_filter.1.push('j');
                    InputResult::Rerender
                }
                (RFocus::Torrents, true, _) => {
                    self.rfocus = RFocus::Details;
                    InputResult::Rerender
                }
                (_, false, _) | (RFocus::Details, true, _) => InputResult::Key(Key::Char('j')),
            },
            Key::Char('k') => match (self.rfocus, self.r_act, self.tfocus) {
                (_, false, Some(TFocus::TrackersFilter)) => {
                    self.trackers_filter.1.push('k');
                    InputResult::Rerender
                }
                (RFocus::TorrentsFilter, true, _) => {
                    self.torrents_filter.1.push('k');
                    InputResult::Rerender
                }
                (RFocus::Details, true, _) => {
                    self.rfocus = RFocus::Torrents;
                    InputResult::Rerender
                }
                (_, false, _) | (_, true, _) => InputResult::Key(Key::Char('k')),
            },
            Key::Up => match (self.rfocus, self.r_act, self.tfocus) {
                (RFocus::Torrents, true, _) => if self.torrent_selected > 0 {
                    self.torrent_selected -= 1;
                    InputResult::Rerender
                } else {
                    InputResult::Key(Key::Up)
                },
                _ => InputResult::Key(Key::Up),
            },
            Key::Down => match (self.rfocus, self.r_act, self.tfocus) {
                (RFocus::Torrents, true, _) => if self.torrent_selected < self.torrents.len() {
                    self.torrent_selected += 1;
                    InputResult::Rerender
                } else {
                    InputResult::Key(Key::Down)
                },
                _ => InputResult::Key(Key::Down),
            },
            Key::Left => match (self.rfocus, self.r_act, self.tfocus) {
                (RFocus::TorrentsFilter, true, _) => {
                    self.torrents_filter.1.cursor_left();
                    InputResult::Rerender
                }
                (_, false, Some(TFocus::TrackersFilter)) => {
                    self.trackers_filter.1.cursor_left();
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Left),
            },
            Key::Right => match (self.rfocus, self.r_act, self.tfocus) {
                (RFocus::TorrentsFilter, true, _) => {
                    self.torrents_filter.1.cursor_right();
                    InputResult::Rerender
                }
                (_, false, Some(TFocus::TrackersFilter)) => {
                    self.trackers_filter.1.cursor_right();
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Right),
            },
            Key::Backspace => match (self.rfocus, self.r_act, self.tfocus) {
                (RFocus::TorrentsFilter, true, _) => {
                    self.torrents_filter.1.backspace();
                    InputResult::Rerender
                }
                (_, false, Some(TFocus::TrackersFilter)) => {
                    self.trackers_filter.1.backspace();
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Backspace),
            },
            Key::Delete => match (self.rfocus, self.r_act, self.tfocus) {
                (RFocus::TorrentsFilter, true, _) => {
                    self.torrents_filter.1.delete();
                    InputResult::Rerender
                }
                (_, false, Some(TFocus::TrackersFilter)) => {
                    self.trackers_filter.1.delete();
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Delete),
            },
            Key::Char(k) => match (self.rfocus, self.r_act, self.tfocus) {
                (RFocus::TorrentsFilter, true, _) => {
                    self.torrents_filter.1.push(k);
                    InputResult::Rerender
                }
                (_, false, Some(TFocus::TrackersFilter)) => {
                    self.trackers_filter.1.push(k);
                    InputResult::Rerender
                }
                (RFocus::Details, true, _) => self.details.input(ctx, Key::Char(k)),
                _ => InputResult::Key(Key::Char(k)),
            },
            ret => InputResult::Key(ret),
        }
    }
}

impl Renderable for TorrentPanel {
    fn name(&self) -> String {
        "torrents".into()
    }
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        let draw_torrents = |target: &mut _, width, height, x, y| {
            for (i, t) in self.torrents.iter().take((height - 1) as _).enumerate() {
                widgets::Text::<_, align::x::Left, align::y::Top>::new(&**t.name
                    .as_ref()
                    .unwrap_or_else(|| &t.path))
                    .render(target, width, 1, x, y + i as u16);
            }
        };
        let draw_trackers = |target: &mut _, width, height, x, y| {
            for (i, t) in self.trackers.iter().take((height - 1) as _).enumerate() {
                let str = if t.error.is_some() {
                    format!(
                        "{}{}{}",
                        color::Fg(color::Red),
                        t.url,
                        color::Fg(color::Reset)
                    )
                } else {
                    format!("{}", t.url)
                };
                widgets::Text::<_, align::x::Left, align::y::Top>::new(str).render(
                    target,
                    width,
                    1,
                    x,
                    y + i as u16,
                );
            }
            let (c_s, c_e) = match (self.r_act, self.tfocus) {
                (false, Some(TFocus::TrackersFilter)) => (
                    format!("{}", color::Fg(color::Cyan)),
                    format!("{}", color::Fg(color::Reset)),
                ),
                _ => ("".into(), "".into()),
            };
            widgets::Text::<_, align::x::Left, align::y::Top>::new(format!(
                "{}{} {}{}",
                c_s,
                match self.trackers_filter.0 {
                    Case::Insensitive => "Filter[i]:",
                    Case::Sensitive => "Filter[s]:",
                },
                self.trackers_filter.1.format_inactive(),
                c_e
            )).render(target, width, 1, x, height + 1);
        };

        match (self.tfocus, self.details.n_tabs() == 0) {
            (None, true) => {
                draw_torrents(target, width, height, x_off, y_off);
            }
            (Some(_), true) => {
                widgets::VSplit::new(
                    &mut widgets::RenderFn::new(draw_trackers) as &mut Renderable,
                    &mut widgets::RenderFn::new(draw_torrents) as &mut Renderable,
                    !self.r_act,
                    0.2,
                ).render(target, width, height, x_off, y_off);
            }
            (None, false) => {
                unimplemented!(); /*
                widgets::BorrowedHSplit::new(
                    &mut widgets::RenderFn::new(draw_torrents),
                    &mut self.details,
                    !(self.rfocus == RFocus::Details),
                    0.65,
                ).render(target, width, height, x_off, y_off);*/
            }
            (Some(_), false) => {
                unimplemented!(); /*
                widgets::BorrowedVSplit::new(
                    &mut widgets::RenderFn::new(draw_trackers),
                    &mut widgets::BorrowedHSplit::new(
                        &mut widgets::IgnoreAll::new(&mut widgets::RenderFn::new(draw_torrents)),
                        &mut self.details,
                        !(self.rfocus == RFocus::Details),
                        0.65,
                    ),
                    !self.r_act,
                    0.2,
                ).render(target, width, height, x_off, y_off);*/
            }
        }
    }
}

impl HandleRpc for TorrentPanel {
    fn rpc(&mut self, ctx: &RpcContext, msg: &SMessage) {
        for d in &mut self.details {
            d.rpc(ctx, msg);
        }
    }
}

pub struct TorrentDetailsPanel {
    torr: Torrent,
}

impl TorrentDetailsPanel {
    pub fn new(torr: Torrent) -> TorrentDetailsPanel {
        TorrentDetailsPanel { torr: torr }
    }
}

impl Component for TorrentDetailsPanel {}

impl Renderable for TorrentDetailsPanel {
    fn name(&self) -> String {
        "torrent details".into()
    }
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        widgets::Text::<_, align::x::Center, align::y::Center>::new("torrent details panel")
            .render(target, width, height, x_off, y_off + height / 2);
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

pub struct StatisticsPanel {}

impl StatisticsPanel {
    fn new() -> StatisticsPanel {
        StatisticsPanel {}
    }
}

impl Component for StatisticsPanel {}

impl Renderable for StatisticsPanel {
    fn name(&self) -> String {
        "statistics".into()
    }
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        widgets::Text::<_, align::x::Center, align::y::Center>::new("statistics panel").render(
            target,
            width,
            height,
            x_off,
            y_off,
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
