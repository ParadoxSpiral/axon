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

use humansize::{file_size_opts as sopt, FileSize};
use synapse_rpc::message::{CMessage, SMessage};
use synapse_rpc::resource::{Resource, ResourceKind, SResourceUpdate, Server, Torrent, Tracker};
use termion::{color, cursor};
use termion::event::Key;
use url::Url;

use std::io::Write;

use rpc::RpcContext;
use utils::{align, Filter};
use utils::align::x::Align;

pub trait Component: Renderable + HandleInput + HandleRpc {}

pub trait Renderable {
    fn name(&self) -> String;
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16);
}

pub trait HandleInput {
    fn input(&mut self, rpc: &RpcContext, k: Key, width: u16, height: u16) -> InputResult;
}

pub trait HandleRpc {
    fn rpc(&mut self, rpc: &RpcContext, msg: &SMessage) -> bool;
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
    fn input(&mut self, ctx: &RpcContext, k: Key, _: u16, _: u16) -> InputResult {
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
            Key::Home => {
                if self.srv_selected {
                    self.server.home();
                } else {
                    self.pass.home();
                }
                InputResult::Rerender
            }
            Key::End => {
                if self.srv_selected {
                    self.server.end();
                } else {
                    self.pass.end();
                }
                InputResult::Rerender
            }
            Key::Char('\n') => if let Err((err, err_name)) = Url::parse(self.server.inner())
                .map_err(|err| (format!("{}", err), "Url"))
                .and_then(|server| {
                    let pass = self.pass.inner();
                    ctx.init(server, pass)
                        .map_err(|err| (format!("{}", err), "RPC"))
                }) {
                let len = err.len();
                InputResult::ReplaceWith(Box::new(widgets::OwnedOverlay::new(
                    widgets::CloseOnInput::new(widgets::IgnoreRpc::new(
                        widgets::Text::<_, align::x::Center, align::y::Top>::new(true, err),
                    )),
                    Box::new(widgets::IgnoreRpcPassInput::new(self.clone())),
                    (len as u16 + 2, 1),
                    color::Red,
                    err_name.to_owned(),
                )) as Box<Component>)
            } else {
                let mut panel = Box::new(MainPanel::new((ctx.next_serial(), ctx.next_serial())));
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

macro_rules! f_push {
    ($s:ident, $c:ident, $v:expr) => {
        $s.filter.1.push($v);
        $s.filter.1.update($c);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    Details,
    Filter,
    Torrents,
}

#[derive(Clone)]
struct MainPanel {
    focus: Focus,
    filter: (bool, Filter),
    torrents: (usize, Vec<Torrent>),
    trackers: Vec<Tracker>,
    trackers_displ: bool,
    details: (usize, Vec<Torrent>),
    server: Server,
}

impl MainPanel {
    fn new(filter_serials: (u64, u64)) -> MainPanel {
        MainPanel {
            focus: Focus::Torrents,
            filter: (false, Filter::new(filter_serials.0, filter_serials.1)),
            torrents: (0, Vec::new()),
            trackers: Vec::new(),
            trackers_displ: false,
            details: (0, Vec::new()),
            server: Default::default(),
        }
    }
}

impl Component for MainPanel {}

impl HandleInput for MainPanel {
    fn input(&mut self, ctx: &RpcContext, k: Key, _: u16, height: u16) -> InputResult {
        match k {
            Key::Char('t') => {
                match (self.focus, self.trackers_displ) {
                    (Focus::Filter, _) => {
                        f_push!(self, ctx, 't');
                    }
                    (_, false) => {
                        self.trackers_displ = true;
                    }
                    (_, true) => {
                        self.trackers_displ = false;
                    }
                }
                InputResult::Rerender
            }
            Key::Char('d') => match self.focus {
                Focus::Filter => {
                    f_push!(self, ctx, 'd');
                    InputResult::Rerender
                }
                Focus::Torrents if !self.torrents.1.is_empty() => {
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
                    self.focus = Focus::Details;
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Char('d')),
            },
            Key::Char('q') => match self.focus {
                Focus::Filter => {
                    f_push!(self, ctx, 'q');
                    InputResult::Rerender
                }
                Focus::Details => {
                    // This is ok, because details only focused when not empty
                    self.details.1.remove(self.details.0);
                    if self.details.0 > 0 {
                        self.details.0 -= 1;
                    }
                    if self.details.1.is_empty() {
                        self.focus = Focus::Torrents;
                    }
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Char('q')),
            },
            Key::Ctrl('s') => match self.focus {
                Focus::Filter => {
                    self.filter.1.cycle();
                    self.filter.1.update(ctx);
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Ctrl('s')),
            },
            Key::Esc => match self.focus {
                Focus::Filter => {
                    self.focus = Focus::Torrents;
                    self.filter.0 = false;
                    self.filter.1.clear();
                    self.filter.1.update(ctx);
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Esc),
            },
            Key::Char('\n') => match self.focus {
                Focus::Torrents if self.filter.0 => {
                    self.focus = Focus::Filter;
                    InputResult::Rerender
                }
                Focus::Filter => {
                    self.focus = Focus::Torrents;
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Char('\n')),
            },
            Key::Char('E') => match self.focus {
                // TODO: Needs testing
                Focus::Torrents | Focus::Details => {
                    if let Some(Some(Some(Some(err)))) = if self.focus == Focus::Torrents {
                        self.torrents.1.get(self.torrents.0)
                    } else {
                        self.details.1.get(self.details.0)
                    }.map(|tor| {
                        self.trackers
                            .iter()
                            .find(|tra| tor.id == tra.torrent_id)
                            .map(|tra| {
                                tra.error.as_ref().map(|e| Some(e.clone())).or_else(|| {
                                    self.trackers
                                        .iter()
                                        .find(|t| tra.url == t.url && t.error.is_some())
                                        .map(|t| t.error.clone())
                                })
                            })
                    }) {
                        let len = err.len();
                        InputResult::ReplaceWith(Box::new(widgets::OwnedOverlay::new(
                            widgets::CloseOnInput::new(widgets::IgnoreRpc::new(
                                widgets::Text::<_, align::x::Center, align::y::Top>::new(true, err),
                            )),
                            // FIXME: There has to be a better way than cloning self
                            Box::new(widgets::IgnoreRpcPassInput::new(self.clone())),
                            (len as u16 + 2, 1),
                            color::Red,
                            "Tracker".to_owned(),
                        )) as Box<Component>)
                    } else {
                        InputResult::Key(Key::Char('E'))
                    }
                }
                Focus::Filter => {
                    f_push!(self, ctx, 'E');
                    InputResult::Rerender
                }
            },
            Key::Char('e') => match self.focus {
                Focus::Torrents | Focus::Details => {
                    if let Some(Some(err)) = if self.focus == Focus::Torrents {
                        self.torrents.1.get(self.torrents.0)
                    } else {
                        self.details.1.get(self.details.0)
                    }.map(|t| t.error.clone())
                    {
                        let len = err.len();
                        InputResult::ReplaceWith(Box::new(widgets::OwnedOverlay::new(
                            widgets::CloseOnInput::new(widgets::IgnoreRpc::new(
                                widgets::Text::<_, align::x::Center, align::y::Top>::new(true, err),
                            )),
                            // FIXME: There has to be a better way than cloning self
                            Box::new(widgets::IgnoreRpcPassInput::new(self.clone())),
                            (len as u16 + 2, 1),
                            color::Red,
                            "Torrent".to_owned(),
                        )) as Box<Component>)
                    } else {
                        InputResult::Key(Key::Char('e'))
                    }
                }
                Focus::Filter => {
                    f_push!(self, ctx, 'e');
                    InputResult::Rerender
                }
            },
            Key::Ctrl('f') => {
                self.focus = Focus::Filter;
                self.filter.0 = true;
                InputResult::Rerender
            }
            Key::Char('H') => match self.focus {
                Focus::Filter => {
                    f_push!(self, ctx, 'H');
                    InputResult::Rerender
                }
                Focus::Details if self.details.0 > 0 => {
                    self.details.0 -= 1;
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Char('H')),
            },
            Key::Char('L') => match self.focus {
                Focus::Filter => {
                    f_push!(self, ctx, 'L');
                    InputResult::Rerender
                }
                Focus::Details => {
                    if self.details.0 < self.details.1.len() - 1 {
                        self.details.0 += 1;
                        InputResult::Rerender
                    } else {
                        InputResult::Key(Key::Char('L'))
                    }
                }
                _ => InputResult::Key(Key::Char('L')),
            },
            Key::Char('J') => match self.focus {
                Focus::Filter => {
                    f_push!(self, ctx, 'J');
                    InputResult::Rerender
                }
                Focus::Torrents if !self.details.1.is_empty() => {
                    self.focus = Focus::Details;
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Char('J')),
            },
            Key::Char('K') => match self.focus {
                Focus::Filter => {
                    f_push!(self, ctx, 'K');
                    InputResult::Rerender
                }
                Focus::Details => {
                    self.focus = Focus::Torrents;
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Char('K')),
            },
            Key::Char('j') => match self.focus {
                Focus::Filter => {
                    f_push!(self, ctx, 'j');
                    InputResult::Rerender
                }
                Focus::Torrents if self.torrents.0 + 1 < self.torrents.1.len() => {
                    self.torrents.0 += 1;
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Char('j')),
            },
            Key::Char('k') => match self.focus {
                Focus::Filter => {
                    f_push!(self, ctx, 'k');
                    InputResult::Rerender
                }
                Focus::Torrents if self.torrents.0 > 0 => {
                    self.torrents.0 -= 1;
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Char('k')),
            },
            Key::Char('h') => match self.focus {
                Focus::Filter => {
                    f_push!(self, ctx, 'h');
                    InputResult::Rerender
                }
                Focus::Details if self.details.0 > 0 => {
                    self.details.0 -= 1;
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Char('h')),
            },
            Key::Char('l') => match self.focus {
                Focus::Filter => {
                    f_push!(self, ctx, 'l');
                    InputResult::Rerender
                }
                Focus::Details if self.details.0 + 1 != self.details.1.len() => {
                    self.details.0 += 1;
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Char('l')),
            },
            Key::Up => match self.focus {
                Focus::Torrents if self.torrents.0 > 0 => {
                    self.torrents.0 -= 1;
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Up),
            },
            Key::Down => match self.focus {
                Focus::Torrents if self.torrents.0 + 1 < self.torrents.1.len() => {
                    self.torrents.0 += 1;
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Down),
            },
            Key::PageUp => match self.focus {
                Focus::Torrents if self.torrents.0 >= height as usize => {
                    self.torrents.0 -= height as usize;
                    InputResult::Rerender
                }
                Focus::Torrents => {
                    self.torrents.0 = 0;
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::PageUp),
            },
            Key::PageDown => match self.focus {
                Focus::Torrents if self.torrents.0 + (height as usize) < self.torrents.1.len() => {
                    self.torrents.0 += height as usize;
                    InputResult::Rerender
                }
                Focus::Torrents => {
                    self.torrents.0 = self.torrents.1.len() - 1;
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::PageDown),
            },
            Key::Left => match self.focus {
                Focus::Details if self.details.0 > 0 => {
                    self.details.0 -= 1;
                    InputResult::Rerender
                }
                Focus::Filter => {
                    self.filter.1.cursor_left();
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Left),
            },
            Key::Right => match self.focus {
                Focus::Details if self.details.0 + 1 != self.details.1.len() => {
                    self.details.0 += 1;
                    InputResult::Rerender
                }
                Focus::Filter => {
                    self.filter.1.cursor_right();
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Right),
            },
            Key::Backspace => match self.focus {
                Focus::Filter => {
                    self.filter.1.backspace();
                    self.filter.1.update(ctx);
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Backspace),
            },
            Key::Delete => match self.focus {
                Focus::Filter => {
                    self.filter.1.delete();
                    self.filter.1.update(ctx);
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Delete),
            },
            Key::Home => match self.focus {
                Focus::Filter => {
                    self.filter.1.home();
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::Home),
            },
            Key::End => match self.focus {
                Focus::Filter => {
                    self.filter.1.end();
                    InputResult::Rerender
                }
                _ => InputResult::Key(Key::End),
            },
            Key::Char(k) => match self.focus {
                Focus::Filter => {
                    f_push!(self, ctx, k);
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
            let ceil = if self.filter.0 { height - 1 } else { height };
            for (i, t) in self.torrents.1.iter().take(ceil as _).enumerate() {
                let (c_s, c_e) = match self.focus {
                    Focus::Torrents if self.torrents.0 == i && t.error.is_some() => (
                        format!("{}{}", color::Fg(color::Cyan), color::Bg(color::Red)),
                        format!("{}{}", color::Fg(color::Reset), color::Bg(color::Reset)),
                    ),
                    Focus::Torrents if self.torrents.0 == i => (
                        format!("{}", color::Fg(color::Cyan)),
                        format!("{}", color::Fg(color::Reset)),
                    ),
                    _ if t.error.is_some() => (
                        format!("{}", color::Fg(color::Red)),
                        format!("{}", color::Fg(color::Reset)),
                    ),
                    _ => ("".into(), "".into()),
                };
                widgets::Text::<_, align::x::Left, align::y::Top>::new(
                    true,
                    format!(
                        "{}{}{}",
                        c_s,
                        &**t.name.as_ref().unwrap_or_else(|| &t.path),
                        c_e
                    ),
                ).render(target, width, 1, x, y + i as u16);
            }
            if self.filter.0 {
                widgets::Text::<_, align::x::Left, align::y::Top>::new(
                    true,
                    match self.focus {
                        Focus::Filter => self.filter.1.format(true),
                        _ => self.filter.1.format(false),
                    },
                ).render(target, width, 1, x, height);
            }
        };
        let draw_trackers = |target: &mut _, width, height, x, y| {
            let sel_tor = match self.focus {
                Focus::Torrents | Focus::Filter => self.torrents.1.get(self.torrents.0),
                Focus::Details => self.details.1.get(self.details.0).map(|d| d),
            };
            let mut trackers: Vec<(_, Tracker, Vec<String>)> =
                Vec::with_capacity(self.trackers.len());
            for t in self.trackers
                .iter()
                .filter(|t| self.torrents.1.iter().any(|tor| tor.id == t.torrent_id))
            {
                if let Some(pos) = trackers.iter().position(|ex| ex.1.url == t.url) {
                    trackers[pos].0 += 1;
                    trackers[pos].2.push(t.torrent_id.clone());
                } else {
                    trackers.push((1, t.clone(), Vec::new()));
                }
            }
            for (i, &(count, ref t, ref tors)) in trackers.iter().take(height as _).enumerate() {
                let matches = sel_tor
                    .map(|s| s.id == t.torrent_id || tors.contains(&s.id))
                    .unwrap_or(false);
                let (c_s, c_e) = match (matches, t.error.is_some()) {
                    (true, true) => (
                        format!("{}{}", color::Fg(color::Cyan), color::Bg(color::Red)),
                        format!("{}{}", color::Fg(color::Reset), color::Bg(color::Reset)),
                    ),
                    (true, false) => (
                        format!("{}", color::Fg(color::Cyan)),
                        format!("{}", color::Fg(color::Reset)),
                    ),
                    (false, true) => (
                        format!("{}", color::Fg(color::Red)),
                        format!("{}", color::Fg(color::Reset)),
                    ),
                    (false, false) => ("".into(), "".into()),
                };
                widgets::Text::<_, align::x::Left, align::y::Top>::new(
                    true,
                    format!(
                        "{}({}) {}{}",
                        c_s,
                        count,
                        t.url
                            .as_ref()
                            .map(|u| u.host_str().unwrap())
                            .unwrap_or_else(|| "?.?"),
                        c_e,
                    ),
                ).render(target, width, 1, x, y + i as u16);
            }
        };
        let draw_details = |target: &mut _, width, height, x, y| {
            let ts = self.details
                .1
                .iter()
                .map(|d| {
                    Box::new(widgets::CloseOnInput::new(widgets::IgnoreRpc::new(
                        // FIXME: Figure out how to avoid allocs
                        TorrentDetailsPanel::new(d.clone()),
                    ))) as Box<Component>
                })
                .collect::<Vec<_>>();
            widgets::Tabs::new(ts, self.details.0).render(target, width, height, x, y);
        };
        let draw_footer = |target: &mut _, width, height, x, y| {
            widgets::Text::<_, align::x::Left, align::y::Top>::new(
                true,
                format!(
                    "Server: {}, {}   {}[{}]↑ {}[{}]↓   \
                     Session: {:.2}, {}↑ {}↓   Lifetime: {:.2}, {}↑ {}↓",
                    ::utils::date_diff_now(self.server.started),
                    self.server.free_space.file_size(sopt::DECIMAL).unwrap(),
                    self.server.rate_up.file_size(sopt::DECIMAL).unwrap(),
                    self.server
                        .throttle_up
                        .map(|t| if t == -1 {
                            "∞".into()
                        } else {
                            t.file_size(sopt::DECIMAL).unwrap()
                        })
                        .unwrap_or("∞".into()),
                    self.server.rate_down.file_size(sopt::DECIMAL).unwrap(),
                    self.server
                        .throttle_down
                        .map(|t| if t == -1 {
                            "∞".into()
                        } else {
                            t.file_size(sopt::DECIMAL).unwrap()
                        })
                        .unwrap_or("∞".into()),
                    if self.server.ses_transferred_down == 0 {
                        1.
                    } else {
                        self.server.ses_transferred_up as f32
                            / self.server.ses_transferred_down as f32
                    },
                    self.server
                        .ses_transferred_up
                        .file_size(sopt::DECIMAL)
                        .unwrap(),
                    self.server
                        .ses_transferred_down
                        .file_size(sopt::DECIMAL)
                        .unwrap(),
                    if self.server.transferred_down == 0 {
                        1.
                    } else {
                        self.server.transferred_up as f32 / self.server.transferred_down as f32
                    },
                    self.server.transferred_up.file_size(sopt::DECIMAL).unwrap(),
                    self.server
                        .transferred_down
                        .file_size(sopt::DECIMAL)
                        .unwrap(),
                ),
            ).render(target, width, height, x, y);
        };

        match (self.trackers_displ, self.details.1.is_empty()) {
            (false, true) => {
                widgets::HSplit::new(
                    &mut widgets::RenderFn::new(draw_torrents) as &mut Renderable,
                    &mut widgets::RenderFn::new(draw_footer) as &mut Renderable,
                    None,
                    widgets::Unit::Lines(height.saturating_sub(2)),
                    true,
                ).render(target, width, height, x_off, y_off);
            }
            (true, true) => {
                widgets::HSplit::new(
                    &mut widgets::VSplit::new(
                        &mut widgets::RenderFn::new(draw_trackers) as &mut Renderable,
                        &mut widgets::RenderFn::new(draw_torrents) as &mut Renderable,
                        None,
                        widgets::Unit::Percent(0.2),
                        true,
                    ) as &mut Renderable,
                    &mut widgets::RenderFn::new(draw_footer) as &mut Renderable,
                    None,
                    widgets::Unit::Lines(height.saturating_sub(2)),
                    true,
                ).render(target, width, height, x_off, y_off);
            }
            (false, false) => {
                widgets::HSplit::new(
                    &mut widgets::HSplit::new(
                        &mut widgets::RenderFn::new(draw_torrents) as &mut Renderable,
                        &mut widgets::RenderFn::new(draw_details) as &mut Renderable,
                        None,
                        widgets::Unit::Lines(height.saturating_sub(8)),
                        false,
                    ) as &mut Renderable,
                    &mut widgets::RenderFn::new(draw_footer) as &mut Renderable,
                    None,
                    widgets::Unit::Lines(height.saturating_sub(2)),
                    true,
                ).render(target, width, height, x_off, y_off);
            }
            (true, false) => {
                widgets::HSplit::new(
                    &mut widgets::VSplit::new(
                        &mut widgets::RenderFn::new(draw_trackers) as &mut Renderable,
                        &mut widgets::HSplit::new(
                            &mut widgets::RenderFn::new(draw_torrents) as &mut Renderable,
                            &mut widgets::RenderFn::new(draw_details) as &mut Renderable,
                            None,
                            widgets::Unit::Lines(height.saturating_sub(8)),
                            false,
                        ) as &mut Renderable,
                        None,
                        widgets::Unit::Percent(0.2),
                        true,
                    ) as &mut Renderable,
                    &mut widgets::RenderFn::new(draw_footer) as &mut Renderable,
                    None,
                    widgets::Unit::Lines(height.saturating_sub(2)),
                    true,
                ).render(target, width, height, x_off, y_off);
            }
        }
    }
}

impl HandleRpc for MainPanel {
    fn rpc(&mut self, _: &RpcContext, msg: &SMessage) -> bool {
        match msg {
            &SMessage::ResourcesRemoved { ref ids, .. } => {
                // FIXME: Some shittiness can go once closure disjoint field borrows land
                let mut i = 0;
                let mut dec = 0;
                let idx = self.torrents.0;
                self.torrents.1.retain(|t| {
                    i += 1;
                    if ids.iter().any(|i| t.id == *i) {
                        if i - 1 == idx && i != 1 {
                            dec += 1;
                        }
                        false
                    } else {
                        true
                    }
                });
                if dec > 0 {
                    self.torrents.0.saturating_sub(dec);
                }

                i = 0;
                dec = 0;
                let idx = self.details.0;
                self.details.1.retain(|t| {
                    i += 1;
                    if ids.iter().any(|i| t.id == *i) {
                        if i - 1 == idx && i != 1 {
                            dec += 1;
                        }
                        false
                    } else {
                        true
                    }
                });
                if dec > 0 {
                    self.torrents.0.saturating_sub(dec);
                }
                if self.details.1.is_empty() && self.focus == Focus::Details {
                    self.focus = Focus::Torrents;
                }

                self.trackers.retain(|t| !ids.iter().any(|i| t.id == *i));

                true
            }
            &SMessage::UpdateResources { ref resources, .. } => {
                for r in resources {
                    match *r {
                        SResourceUpdate::Resource(ref res) => {
                            if let Resource::Torrent(ref t) = **res {
                                ::utils::insert_sorted(&mut self.torrents.1, t.clone(), |t, ex| {
                                    t.name
                                        .as_ref()
                                        .map(|n| n.to_lowercase())
                                        .cmp(&ex.name.as_ref().map(|n| n.to_lowercase()))
                                });
                            } else if let Resource::Tracker(ref t) = **res {
                                ::utils::insert_sorted(&mut self.trackers, t.clone(), |t, ex| {
                                    t.url.as_ref().unwrap().host_str().unwrap().cmp(&ex.url
                                        .as_ref()
                                        .unwrap()
                                        .host_str()
                                        .unwrap())
                                });
                            } else if let Resource::Server(ref s) = **res {
                                self.server = s.clone();
                            }
                        }
                        _ => {
                            for t in &mut self.torrents.1 {
                                t.update(r.clone());
                            }
                            for t in &mut self.details.1 {
                                t.update(r.clone());
                            }
                            for t in &mut self.trackers {
                                t.update(r.clone());
                            }
                            self.server.update(r.clone());
                        }
                    }
                }
                true
            }
            _ => false,
        }
    }
    fn init(&mut self, ctx: &RpcContext) {
        ctx.send(CMessage::FilterSubscribe {
            serial: ctx.next_serial(),
            kind: ResourceKind::Server,
            criteria: Vec::new(),
        });
        self.filter.1.init(ctx);
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
        if height >= 1 {
            widgets::Text::<_, align::x::Left, align::y::Top>::new(
                true,
                format!(
                    "{}    {}    Created: {} ago    Modified: {} ago",
                    self.torr.status.as_str(),
                    if self.torr.sequential {
                        "Sequential"
                    } else {
                        "Unordered"
                    },
                    ::utils::date_diff_now(self.torr.created),
                    ::utils::date_diff_now(self.torr.modified),
                ),
            ).render(target, width, 1, x_off, y_off);
        }

        if height >= 2 {
            widgets::Text::<_, align::x::Left, align::y::Top>::new(
                true,
                format!(
                    "Rate up: {}[{}]    Rate down: {}[{}]    Upped: {}    Downed: {}",
                    self.torr.rate_up.file_size(sopt::DECIMAL).unwrap(),
                    self.torr
                        .throttle_up
                        .map(|t| if t == -1 {
                            "∞".into()
                        } else {
                            t.file_size(sopt::DECIMAL).unwrap()
                        })
                        .unwrap_or("srv".into()),
                    self.torr.rate_down.file_size(sopt::DECIMAL).unwrap(),
                    self.torr
                        .throttle_down
                        .map(|t| if t == -1 {
                            "∞".into()
                        } else {
                            t.file_size(sopt::DECIMAL).unwrap()
                        })
                        .unwrap_or("srv".into()),
                    self.torr.transferred_up.file_size(sopt::DECIMAL).unwrap(),
                    self.torr.transferred_down.file_size(sopt::DECIMAL).unwrap(),
                ),
            ).render(target, width, 1, x_off, y_off + 1);
        }

        if height >= 3 {
            widgets::Text::<_, align::x::Left, align::y::Top>::new(
                true,
                format!(
                    "Size: {}    Progress: {}%    Availability: {}%    Priority: {}",
                    self.torr
                        .size
                        .map(|p| p.file_size(sopt::DECIMAL).unwrap())
                        .unwrap_or("?".into()),
                    (self.torr.progress * 100.).round(),
                    (self.torr.availability * 100.).round(),
                    self.torr.priority,
                ),
            ).render(target, width, 1, x_off, y_off + 2);
        }

        if height >= 4 {
            widgets::Text::<_, align::x::Left, align::y::Top>::new(
                true,
                format!(
                    "Files: {}    Pieces: {}    P-size: {}    Peers: {}    Trackers: {}",
                    self.torr
                        .files
                        .map(|f| format!("{}", f))
                        .unwrap_or("?".into()),
                    self.torr
                        .pieces
                        .map(|p| format!("{}", p))
                        .unwrap_or("?".into()),
                    self.torr
                        .piece_size
                        .map(|p| p.file_size(sopt::DECIMAL).unwrap())
                        .unwrap_or("?".into()),
                    self.torr.peers,
                    self.torr.trackers,
                ),
            ).render(target, width, 1, x_off, y_off + 3);
        }

        if height >= 5 {
            widgets::Text::<_, align::x::Left, align::y::Top>::new(
                true,
                format!("Path: {}", self.torr.path,),
            ).render(target, width, 1, x_off, y_off + 4);
        }
    }
}
