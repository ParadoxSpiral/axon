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
    fn init(&mut self, rpc: &RpcContext);
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
                let mut panel = Box::new(widgets::Tabs::new(
                    vec![Box::new(MainPanel::new()), Box::new(StatisticsPanel::new())],
                    0,
                ));
                // Init rpc subscribes etc
                panel.init(ctx);
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

struct MainPanel {
    r_act: bool,
    rfocus: RFocus,
    tfocus: Option<TFocus>,
    torrents: (usize, Vec<Torrent>),
    torrents_filter: (bool, Case, widgets::Input),
    trackers: Vec<Tracker>,
    trackers_filter: (bool, Case, widgets::Input),
    details: (usize, Vec<Torrent>),
}

impl MainPanel {
    fn new() -> MainPanel {
        MainPanel {
            rfocus: RFocus::Torrents,
            r_act: true,
            tfocus: None,
            torrents: (0, Vec::new()),
            torrents_filter: (false, Case::Insensitive, widgets::Input::from("", 1)),
            trackers: Vec::new(),
            trackers_filter: (false, Case::Insensitive, widgets::Input::from("", 1)),
            details: (0, Vec::new()),
        }
    }
}

impl Component for MainPanel {}

impl HandleInput for MainPanel {
    fn input(&mut self, ctx: &RpcContext, k: Key) -> InputResult {
        match k {
            Key::Char('t') => {
                match (self.rfocus, self.r_act, self.tfocus) {
                    (RFocus::TorrentsFilter, true, _) => {
                        self.torrents_filter.2.push('t');
                    }
                    (_, false, Some(TFocus::TrackersFilter)) => {
                        self.trackers_filter.2.push('t');
                    }
                    (_, _, Some(_)) => {
                        self.tfocus = None;
                        self.r_act = true;
                    }
                    (_, _, None) => {
                        self.tfocus = Some(TFocus::Trackers);
                        self.r_act = false;
                    }
                }
                InputResult::Rerender
            }
            Key::Char('d') => match (self.rfocus, self.r_act, self.tfocus) {
                (RFocus::Torrents, true, _) => if !self.torrents.1.is_empty() {
                    if let Some(pos) = self.details
                        .1
                        .iter()
                        .position(|dt| dt.id == self.torrents.1[self.torrents.0].id)
                    {
                        self.details.0 = pos;
                    } else {
                        self.details
                            .1
                            .push(self.torrents.1[self.torrents.0].clone());
                        self.details.0 = self.details.1.len() - 1;
                    }
                    InputResult::Rerender
                } else {
                    InputResult::Key(Key::Char('d'))
                },
                (RFocus::TorrentsFilter, true, _) => {
                    self.torrents_filter.2.push('d');
                    InputResult::Rerender
                }
                (_, false, Some(TFocus::TrackersFilter)) => {
                    self.trackers_filter.2.push('d');
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Char('d')),
            },
            Key::Char('q') => match (self.rfocus, self.r_act, self.tfocus) {
                (RFocus::Details, true, _) => {
                    // This is ok, because details only focused when not empty
                    self.details.1.remove(self.details.0);
                    if self.details.0 > 0 {
                        self.details.0 -= 1;
                    }
                    if self.details.1.is_empty() {
                        self.rfocus = RFocus::Torrents;
                    }
                    InputResult::Rerender
                }
                (RFocus::TorrentsFilter, true, _) => {
                    self.torrents_filter.2.push('q');
                    InputResult::Rerender
                }
                (_, false, Some(TFocus::TrackersFilter)) => {
                    self.trackers_filter.2.push('q');
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Char('q')),
            },
            Key::Ctrl('s') => match (self.rfocus, self.r_act, self.tfocus) {
                (RFocus::TorrentsFilter, true, _) | (RFocus::Torrents, true, _) => {
                    if self.torrents_filter.1 == Case::Sensitive {
                        self.torrents_filter.1 = Case::Insensitive;
                    } else {
                        self.torrents_filter.1 = Case::Sensitive;
                    }
                    InputResult::Rerender
                }
                (_, false, _) => {
                    if self.trackers_filter.1 == Case::Sensitive {
                        self.trackers_filter.1 = Case::Insensitive;
                    } else {
                        self.trackers_filter.1 = Case::Sensitive;
                    }
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Ctrl('s')),
            },
            Key::Esc => match (self.rfocus, self.r_act, self.tfocus) {
                (RFocus::TorrentsFilter, true, _) => {
                    self.rfocus = RFocus::Torrents;
                    self.torrents_filter.0 = false;
                    self.torrents_filter.2.clear();
                    InputResult::Rerender
                }
                (_, false, Some(TFocus::TrackersFilter)) => {
                    self.tfocus = Some(TFocus::Trackers);
                    self.trackers_filter.0 = false;
                    self.trackers_filter.2.clear();
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Esc),
            },
            Key::Char('\n') => match (self.rfocus, self.r_act, self.tfocus, self.torrents_filter.0, self.trackers_filter.0) {
                (RFocus::Torrents, true, _, true, _) => {
                    self.rfocus = RFocus::TorrentsFilter;
                    InputResult::Rerender
                }
                (RFocus::TorrentsFilter, true, _, true, _) => {
                    self.rfocus = RFocus::Torrents;
                    InputResult::Rerender
                }
                (_, false, Some(TFocus::Trackers), _, true) => {
                    self.tfocus = Some(TFocus::TrackersFilter);
                    InputResult::Rerender
                }
                (_, false, Some(TFocus::TrackersFilter), _, true) => {
                    self.tfocus = Some(TFocus::Trackers);
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Esc),
            },
            Key::Ctrl('f') => match (self.rfocus, self.r_act, self.tfocus) {
                (RFocus::Torrents, true, _) => {
                    self.rfocus = RFocus::TorrentsFilter;
                    self.torrents_filter.0 = true;
                    InputResult::Rerender
                }
                (_, false, Some(TFocus::Trackers)) => {
                    self.tfocus = Some(TFocus::TrackersFilter);
                    self.trackers_filter.0 = true;
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Char('\n')),
            },
            Key::Char('h') => match (self.rfocus, self.r_act, self.tfocus) {
                (RFocus::TorrentsFilter, true, _) => {
                    self.torrents_filter.2.push('h');
                    InputResult::Rerender
                }
                (RFocus::Details, true, _) => {
                    if self.details.0 > 0 {
                        self.details.0 -= 1;
                        InputResult::Rerender
                    } else {
                        InputResult::Key(Key::Char('h'))
                    }
                }
                (_, false, Some(TFocus::TrackersFilter)) => {
                    self.trackers_filter.2.push('h');
                    InputResult::Rerender
                }
                (_, true, Some(_)) => {
                    self.r_act = false;
                    InputResult::Rerender
                }
                (_, _, None) | (_, false, _) => InputResult::Key(Key::Char('h')),
            },
            Key::Char('l') => match (self.rfocus, self.r_act, self.tfocus) {
                (_, false, Some(TFocus::TrackersFilter)) => {
                    self.trackers_filter.2.push('l');
                    InputResult::Rerender
                }
                (RFocus::Details, true, _) => {
                    if self.details.0 < self.details.1.len() - 1 {
                        self.details.0 += 1;
                        InputResult::Rerender
                    } else {
                        InputResult::Key(Key::Char('l'))
                    }
                }
                (RFocus::TorrentsFilter, true, _) => {
                    self.torrents_filter.2.push('l');
                    InputResult::Rerender
                }
                (_, false, _) => {
                    self.r_act = true;
                    InputResult::Rerender
                }
                (_, true, _) => InputResult::Key(Key::Char('l')),
            },
            Key::Char('j') => match (self.rfocus, self.r_act, self.tfocus) {
                (_, false, Some(TFocus::TrackersFilter)) => {
                    self.trackers_filter.2.push('j');
                    InputResult::Rerender
                }
                (RFocus::TorrentsFilter, true, _) => {
                    self.torrents_filter.2.push('j');
                    InputResult::Rerender
                }
                (RFocus::Torrents, true, _) => {
                    if !self.details.1.is_empty() {
                        self.rfocus = RFocus::Details;
                    }
                    InputResult::Rerender
                }
                (_, false, _) | (RFocus::Details, true, _) => InputResult::Key(Key::Char('j')),
            },
            Key::Char('k') => match (self.rfocus, self.r_act, self.tfocus) {
                (_, false, Some(TFocus::TrackersFilter)) => {
                    self.trackers_filter.2.push('k');
                    InputResult::Rerender
                }
                (RFocus::TorrentsFilter, true, _) => {
                    self.torrents_filter.2.push('k');
                    InputResult::Rerender
                }
                (RFocus::Details, true, _) => {
                    self.rfocus = RFocus::Torrents;
                    InputResult::Rerender
                }
                (_, false, _) | (_, true, _) => InputResult::Key(Key::Char('k')),
            },
            Key::Up => match (self.rfocus, self.r_act, self.tfocus) {
                (RFocus::Torrents, true, _) => if self.torrents.0 > 0 {
                    self.torrents.0 -= 1;
                    InputResult::Rerender
                } else {
                    InputResult::Key(Key::Up)
                },
                _ => InputResult::Key(Key::Up),
            },
            Key::Down => match (self.rfocus, self.r_act, self.tfocus) {
                (RFocus::Torrents, true, _) => if self.torrents.0 + 1 < self.torrents.1.len() {
                    self.torrents.0 += 1;
                    InputResult::Rerender
                } else {
                    InputResult::Key(Key::Down)
                },
                _ => InputResult::Key(Key::Down),
            },
            Key::Left => match (self.rfocus, self.r_act, self.tfocus) {
                (RFocus::TorrentsFilter, true, _) => {
                    self.torrents_filter.2.cursor_left();
                    InputResult::Rerender
                }
                (_, false, Some(TFocus::TrackersFilter)) => {
                    self.trackers_filter.2.cursor_left();
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Left),
            },
            Key::Right => match (self.rfocus, self.r_act, self.tfocus) {
                (RFocus::TorrentsFilter, true, _) => {
                    self.torrents_filter.2.cursor_right();
                    InputResult::Rerender
                }
                (_, false, Some(TFocus::TrackersFilter)) => {
                    self.trackers_filter.2.cursor_right();
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Right),
            },
            Key::Backspace => match (self.rfocus, self.r_act, self.tfocus) {
                (RFocus::TorrentsFilter, true, _) => {
                    self.torrents_filter.2.backspace();
                    InputResult::Rerender
                }
                (_, false, Some(TFocus::TrackersFilter)) => {
                    self.trackers_filter.2.backspace();
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Backspace),
            },
            Key::Delete => match (self.rfocus, self.r_act, self.tfocus) {
                (RFocus::TorrentsFilter, true, _) => {
                    self.torrents_filter.2.delete();
                    InputResult::Rerender
                }
                (_, false, Some(TFocus::TrackersFilter)) => {
                    self.trackers_filter.2.delete();
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Delete),
            },
            Key::Char(k) => match (self.rfocus, self.r_act, self.tfocus) {
                (RFocus::TorrentsFilter, true, _) => {
                    self.torrents_filter.2.push(k);
                    InputResult::Rerender
                }
                (_, false, Some(TFocus::TrackersFilter)) => {
                    self.trackers_filter.2.push(k);
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Char(k)),
            },
            ret => InputResult::Key(ret),
        }
    }
}

impl Renderable for MainPanel {
    fn name(&self) -> String {
        "torrents".into()
    }
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        let draw_torrents = |target: &mut _, width, height, x, y| {
            let ceil = if self.torrents_filter.0 {
                height - 1
            } else {
                height
            };
            for (i, t) in self.torrents.1.iter().take(ceil as _).enumerate() {
                if self.torrents.0 == i {
                    let (c_s, c_e) = match (self.rfocus, self.r_act) {
                        (RFocus::Torrents, true) => (
                            format!("{}", color::Fg(color::Cyan)),
                            format!("{}", color::Fg(color::Reset)),
                        ),
                        _ => ("".into(), "".into()),
                    };
                    widgets::Text::<_, align::x::Left, align::y::Top>::new(format!(
                        "{}{}{}",
                        c_s,
                        &**t.name.as_ref().unwrap_or_else(|| &t.path),
                        c_e
                    )).render(target, width, 1, x, y + i as u16);
                } else {
                    widgets::Text::<_, align::x::Left, align::y::Top>::new(&**t.name
                        .as_ref()
                        .unwrap_or_else(|| &t.path))
                        .render(target, width, 1, x, y + i as u16);
                }
            }
            if self.torrents_filter.0 {
                let (c_s, c_e) = match (self.rfocus, self.r_act) {
                    (RFocus::TorrentsFilter, true) => (
                        format!("{}", color::Fg(color::Cyan)),
                        format!("{}", color::Fg(color::Reset)),
                    ),
                    _ => ("".into(), "".into()),
                };
                widgets::Text::<_, align::x::Left, align::y::Top>::new(format!(
                    "{}{} {}{}",
                    c_s,
                    match self.torrents_filter.1 {
                        Case::Insensitive => "Filter[i]:",
                        Case::Sensitive => "Filter[s]:",
                    },
                    self.torrents_filter.2.format_inactive(),
                    c_e
                )).render(target, width, 1, x, height + 1);
            }
        };
        let draw_trackers = |target: &mut _, width, height, x, y| {
            let ceil = if self.trackers_filter.0 {
                height - 1
            } else {
                height
            };
            for (i, t) in self.trackers.iter().take(ceil as _).enumerate() {
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
            if self.trackers_filter.0 {
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
                    match self.trackers_filter.1 {
                        Case::Insensitive => "Filter[i]:",
                        Case::Sensitive => "Filter[s]:",
                    },
                    self.trackers_filter.2.inner(),
                    c_e
                )).render(target, width, 1, x, height + 1);
            }
        };
        let draw_details = |target: &mut _, width, height, x, y| {
            let ts = self.details
                .1
                .iter()
                .map(|d| {
                    Box::new(widgets::CloseOnInput::new(widgets::IgnoreRpc::new(
                        // FIXME: Figure out how to avoid the clone, but it might very well not be
                        // possible or even really needed
                        TorrentDetailsPanel::new(d.clone()),
                    ))) as Box<Component>
                })
                .collect::<Vec<_>>();
            widgets::Tabs::new(ts, self.details.0).render(target, width, height, x, y);
        };

        match (self.tfocus, self.details.1.is_empty()) {
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
                widgets::HSplit::new(
                    &mut widgets::RenderFn::new(draw_torrents) as &mut Renderable,
                    &mut widgets::RenderFn::new(draw_details) as &mut Renderable,
                    !(self.rfocus == RFocus::Details),
                    0.65,
                ).render(target, width, height, x_off, y_off);
            }
            (Some(_), false) => {
                widgets::VSplit::new(
                    &mut widgets::RenderFn::new(draw_trackers) as &mut Renderable,
                    &mut widgets::HSplit::new(
                        &mut widgets::RenderFn::new(draw_torrents) as &mut Renderable,
                        &mut widgets::RenderFn::new(draw_details) as &mut Renderable,
                        !(self.rfocus == RFocus::Details),
                        0.65,
                    ) as &mut Renderable,
                    !self.r_act,
                    0.2,
                ).render(target, width, height, x_off, y_off);
            }
        }
    }
}

impl HandleRpc for MainPanel {
    fn rpc(&mut self, ctx: &RpcContext, msg: &SMessage) {
        match msg {
            &SMessage::ResourcesRemoved { ref ids, .. } => {
                // FIXME: This shittiness can go once closure disjoint field borrows land
                let mut i = 0;
                let mut dec = false;
                let idx = self.torrents.0;
                self.torrents.1.retain(|t| {
                    i += 1;
                    if ids.iter().any(|i| t.id == *i) {
                        if i - 1 == idx && i != 1 {
                            dec = true;
                        }
                        false
                    } else {
                        true
                    }
                });
                if dec {
                    self.torrents.0 -= 1;
                }

                i = 0;
                dec = false;
                let idx = self.details.0;
                self.details.1.retain(|t| {
                    i += 1;
                    if ids.iter().any(|i| t.id == *i) {
                        if i - 1 == idx && i != 1 {
                            dec = true;
                        }
                        false
                    } else {
                        true
                    }
                });
                if dec {
                    self.details.0 -= 1;
                }

                i = 0;
                self.trackers.retain(|t| {
                    i += 1;
                    if ids.iter().any(|i| t.id == *i) {
                        false
                    } else {
                        true
                    }
                });
            }
            &SMessage::UpdateResources { ref resources } => for r in resources {
                match *r {
                    SResourceUpdate::Resource(ref res) => if let Resource::Torrent(ref t) = **res {
                        self.torrents.1.push(t.clone());
                    } else if let Resource::Tracker(ref t) = **res {
                        self.trackers.push(t.clone());
                    },
                    ref upd => {
                        for t in &mut self.torrents.1 {
                            ::utils::update_torrent(t, upd);
                        }
                        for t in &mut self.details.1 {
                            ::utils::update_torrent(t, upd);
                        }
                        for t in &mut self.trackers {
                            ::utils::update_tracker(t, upd);
                        }
                    }
                }
            },
            _ => {}
        }
    }
    fn init(&mut self, ctx: &RpcContext) {
        ctx.send(CMessage::FilterSubscribe {
            serial: ctx.next_serial(),
            kind: ResourceKind::Torrent,
            criteria: Vec::new()
            ,
        });
        ctx.send(CMessage::FilterSubscribe {
            serial: ctx.next_serial(),
            kind: ResourceKind::Tracker,
            criteria: Vec::new()
            ,
        });
    }
}

pub struct TorrentDetailsPanel {
    torr: Torrent,
}
impl TorrentDetailsPanel {
    fn new(torr: Torrent) -> TorrentDetailsPanel {
        TorrentDetailsPanel { torr }
    }
}

impl Renderable for TorrentDetailsPanel {
    fn name(&self) -> String {
        self.torr
            .name
            .as_ref()
            .unwrap_or_else(|| &self.torr.path)
            .clone()
    }
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        widgets::Text::<_, align::x::Center, align::y::Center>::new(format!(
            "details of: {}",
            self.name()
        )).render(target, width, height, x_off, y_off);
    }
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
    fn init(&mut self, _: &RpcContext) {
        // TODO: enumerate initial set + subscribe
    }
}
