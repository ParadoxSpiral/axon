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

use natord;
use synapse_rpc::message::{CMessage, SMessage};
use synapse_rpc::resource::{Resource, ResourceKind, SResourceUpdate, Server, Torrent, Tracker};
use termion::{color, cursor};
use termion::event::Key;
use url::Url;

use std::borrow::Cow;
use std::cmp;
use std::collections::HashMap;
use std::cmp::Ordering;
use std::io::Write;

use config::CONFIG;
use rpc::RpcContext;
use super::{widgets, Component, Renderable, HandleInput, HandleRpc, InputResult};
use utils::align;
use utils::align::x::Align;
use utils::filter::{self, Filter};
use utils::fmt::{self, FormatSize};

#[derive(Clone)]
pub struct LoginPanel {
    server: widgets::Input,
    pass: widgets::PasswordInput,
    srv_selected: bool,
    error: Option<(String, &'static str)>,
}

impl LoginPanel {
    pub fn new() -> LoginPanel {
        LoginPanel {
            server: CONFIG.server.as_ref().map(|s| widgets::Input::from(s.clone(), s.len() + 1)).unwrap_or_else(|| widgets::Input::from("ws://:8412".into(), 6)),
            pass: CONFIG.pass.as_ref().map(|s| widgets::PasswordInput::from(s.clone(), s.len() + 1)).unwrap_or_else(|| widgets::PasswordInput::with_capacity(20)),
            srv_selected: true,
            error: None,
        }
    }

    pub fn try_connect(&self, ctx: &RpcContext) -> Result<MainPanel, (String, &'static str)> {
        Url::parse(self.server.inner())
        .map_err(|err| (format!("{}", err), "Url"))
        .and_then(|server| {
            let pass = self.pass.inner();
            ctx.wait_init(server, pass.to_owned())
            .map(|_| {
                MainPanel::new(ctx)
            })
            .map_err(|err| {
                (format!("{}", err), "RPC")
            })
        })
    }
}

impl Renderable for LoginPanel {
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, _: u16, _: u16) {
        let draw = |target: &mut Vec<u8>, width, height, _, _| {
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
        };

        if let Some((ref e, ref name)) = self.error {
            widgets::BorrowedOverlay::new(
                &mut widgets::Text::<_, align::x::Center, align::y::Top>::new(true, &**e),
                &mut widgets::RenderFn::new(draw),
                (cmp::max(name.len(), e.len()) as u16 + 2, 1),
                Some(&color::Red),
                Some(*name),
            ).render(target, width, height, 1, 1);
        } else {
            draw(target, width, height, 1, 1);
        }
    }

    fn name(&self) -> String {
        "login".to_owned()
    }
}

impl HandleInput for LoginPanel {
    fn input(&mut self, ctx: &RpcContext, k: Key, _: u16, _: u16) -> InputResult {
        if self.error.is_some() {
            self.error = None;
            return InputResult::Rerender;
        }
        match k {
            Key::Home => if self.srv_selected {
                self.server.home();
            } else {
                self.pass.home();
            },

            Key::End => if self.srv_selected {
                self.server.end();
            } else {
                self.pass.end();
            },

            Key::Down | Key::Up | Key::Char('\t') => {
                self.srv_selected = !self.srv_selected;
            }

            Key::Left => if self.srv_selected {
                self.server.cursor_left();
            } else {
                self.pass.cursor_left();
            },

            Key::Right => if self.srv_selected {
                self.server.cursor_right();
            } else {
                self.pass.cursor_right();
            },

            Key::Backspace => if self.srv_selected {
                self.server.backspace();
            } else {
                self.pass.backspace();
            },

            Key::Delete => if self.srv_selected {
                self.server.delete();
            } else {
                self.pass.delete();
            },

            Key::Char('\n') => {
                match self.try_connect(ctx) {
                    Ok(main) => {
                        return InputResult::ReplaceWith(Box::new(main) as Box<Component>);
                    }
                    Err(e) => {
                        self.error = Some(e);
                    }
                }
            }

            Key::Char(c) => if self.srv_selected {
                self.server.push(c);
            } else {
                self.pass.push(c);
            },
            _ => {
                return InputResult::Key(k);
            }
        }
        InputResult::Rerender
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    Details,
    Filter,
    Torrents,
}

#[derive(Clone)]
pub struct MainPanel {
    focus: Focus,
    filter: Option<Filter>,
    // lower bound of torrent selection,  current pos, _
    torrents: (usize, usize, Vec<Torrent>),
    // tracker base, Vec<(tracker id, torrent_id, optional error)>
    trackers: Vec<(Tracker, Vec<(String, String, Option<String>)>)>,
    trackers_displ: bool,
    details: (usize, Vec<TorrentDetailsPanel>),
    server: Server,
    server_version: String,
}

impl MainPanel {
    fn new(ctx: &RpcContext) -> MainPanel {
        let mut p = MainPanel {
            focus: Focus::Torrents,
            filter: None,
            torrents: (0, 0, Vec::new()),
            trackers: Vec::new(),
            trackers_displ: false,
            details: (0, Vec::new()),
            server: Default::default(),
            server_version: "?.?".to_owned(),
        };
        p.init(ctx);
        p
    }
}

impl Component for MainPanel {}

impl HandleInput for MainPanel {
    fn input(&mut self, ctx: &RpcContext, k: Key, width: u16, height: u16) -> InputResult {
        // - 2 because of the server footer
        let torr_height = height.saturating_sub(2) as usize;

        match (k, self.focus) {
            // Special keys
            (Key::Ctrl('f'), Focus::Filter) => {
                self.focus = Focus::Torrents;
                self.filter.as_ref().unwrap().reset(ctx);
                self.filter = None;
            }
            (Key::Ctrl('f'), _) => {
                self.focus = Focus::Filter;
                self.filter = Some(Filter::new());
            }

            (Key::Esc, Focus::Filter) => {
                self.focus = Focus::Torrents;
            }

            // Movement Keys
            (Key::Home, Focus::Torrents) => {
                self.torrents.0 = 0;
                self.torrents.1 = 0;
            }
            (Key::Home, Focus::Details) => {
                self.details.0 = 0;
            }

            (Key::End, Focus::Torrents) => {
                let l = self.torrents.2.len();
                self.torrents.0 = l.saturating_sub(torr_height);
                self.torrents.1 = l.saturating_sub(1);
            }
            (Key::End, Focus::Details) => {
                self.details.0 = self.details.1.len() - 1;
            }

            (Key::PageUp, Focus::Torrents) if self.torrents.1 < torr_height => {
                self.torrents.0 = 0;
                self.torrents.1 = 0;
            }
            (Key::PageUp, Focus::Torrents) => {
                if self.torrents.0 < torr_height {
                    self.torrents.0 = 0;
                } else {
                    self.torrents.0 -= torr_height;
                }
                self.torrents.1 -= torr_height;
            }

            (Key::PageDown, Focus::Torrents)
                if self.torrents.1 + torr_height >= self.torrents.2.len() =>
            {
                let l = self.torrents.2.len();
                self.torrents.0 = l.saturating_sub(torr_height);
                self.torrents.1 = l.saturating_sub(1);
            }
            (Key::PageDown, Focus::Torrents) => {
                if self.torrents.0 + 2 * torr_height >= self.torrents.2.len() {
                    self.torrents.0 = self.torrents.2.len().saturating_sub(torr_height);
                } else {
                    self.torrents.0 += torr_height;
                }
                self.torrents.1 += torr_height;
            }

            (Key::Up, Focus::Torrents) | (Key::Char('k'), Focus::Torrents)
                if self.torrents.1 > 0 =>
            {
                if self.torrents.0 == self.torrents.1 {
                    self.torrents.0 -= 1;
                }
                self.torrents.1 -= 1;
            }

            (Key::Down, Focus::Torrents) | (Key::Char('j'), Focus::Torrents)
                if self.torrents.1 + 1 < self.torrents.2.len() =>
            {
                if self.torrents.0 + torr_height.saturating_sub(1) == self.torrents.1 {
                    self.torrents.0 += 1;
                }
                self.torrents.1 += 1;
            }

            (Key::Left, Focus::Details) | (Key::Char('h'), Focus::Details)
                if self.details.0 > 0 =>
            {
                self.details.0 -= 1;
            }

            (Key::Right, Focus::Details) | (Key::Char('l'), Focus::Details)
                if self.details.0 + 1 != self.details.1.len() =>
            {
                self.details.0 += 1;
            }

            // Key::Char
            (Key::Char('\n'), Focus::Torrents) if self.filter.is_some() => {
                self.focus = Focus::Filter;
            }

            (Key::Char('d'), Focus::Torrents) if !self.torrents.2.is_empty() => {
                if let Some(pos) = self.details
                    .1
                    .iter()
                    .position(|dt| dt.inner().id == self.torrents.2[self.torrents.1].id)
                {
                    self.details.0 = pos;
                } else {
                    self.details.1.push(TorrentDetailsPanel::new(
                        self.torrents.2[self.torrents.1].clone(),
                    ));
                    self.details.0 = self.details.1.len() - 1;
                }
                self.focus = Focus::Details;
            }

            (Key::Char('e'), Focus::Torrents) | (Key::Char('e'), Focus::Details) => {
                return if self.focus == Focus::Torrents {
                    self.torrents.2.get(self.torrents.1)
                } else {
                    self.details.1.get(self.details.0).map(|d| d.inner())
                }.and_then(|t| {
                    let mut tree = Vec::new();
                    let mut len = "Errors".len() as u16;
                    t.error.as_ref().map(|e| {
                        tree.push(e.clone());
                        len = cmp::max(len, e.len() as u16);
                    });
                    for &(ref base, ref others) in self.trackers.iter().filter(|tra| {
                        t.tracker_urls
                            .iter()
                            .any(|tu| &*tu == tra.0.url.host_str().unwrap())
                    }) {
                        let mut other_errs = others
                            .iter()
                            .filter(|&&(_, ref id, ref err)| t.id == *id && err.is_some())
                            .map(|&(_, _, ref err)| err.as_ref().unwrap().clone())
                            .peekable();
                        if base.error.is_some() && base.torrent_id == t.id {
                            let s = format!(
                                "{}: {}",
                                base.url.host_str().unwrap(),
                                base.error.as_ref().unwrap().clone(),
                            );
                            len = cmp::max(len, s.len() as u16);
                            tree.push(s);
                        } else if other_errs.peek().is_some() {
                            let s = format!(
                                "{}: {}",
                                base.url.host_str().unwrap(),
                                other_errs.next().unwrap()
                            );
                            len = cmp::max(len, s.len() as u16);
                            tree.push(s);
                        }
                        for e in other_errs {
                            let s = format!(" {}", e);
                            len = cmp::max(len, s.len() as u16);
                            tree.push(s);
                        }
                    }
                    if tree.is_empty() {
                        return None;
                    }

                    let draw = |target: &mut _, width, _, x, y, state: &mut Vec<String>| {
                        for (i, e) in state.iter().enumerate() {
                            widgets::Text::<_, align::x::Left, align::y::Top>::new(true, &**e)
                                .render(target, width, 1, x, y + i as u16);
                        }
                    };

                    Some(InputResult::ReplaceWith(
                        Box::new(widgets::OwnedOverlay::new(
                            widgets::CloseOnInput::new(widgets::IgnoreRpc::new(
                                widgets::RenderStateFn::new(draw, tree),
                            )),
                            Box::new(self.clone()),
                            (len, 1),
                            color::Red,
                            "Errors".to_owned(),
                        )) as Box<Component>,
                    ))
                })
                    .unwrap_or(InputResult::Key(Key::Char('e')));
            }

            (Key::Char('J'), Focus::Torrents) if !self.details.1.is_empty() => {
                self.focus = Focus::Details;
            }

            (Key::Char('K'), Focus::Details) => {
                self.focus = Focus::Torrents;
            }

            (Key::Char('q'), Focus::Details) => {
                // This is ok, because details only focused when not empty
                // FIXME: NLL
                let i = self.details.0;
                self.details.1.remove(i);
                self.details.0.saturating_sub(1);
                if self.details.1.is_empty() {
                    self.focus = Focus::Torrents;
                }
            }

            (Key::Char('t'), Focus::Torrents) | (Key::Char('t'), Focus::Details) => {
                self.trackers_displ = !self.trackers_displ;
            }

            // Catch all filter input
            (k, Focus::Filter) => {
                return self.filter.as_mut().unwrap().input(ctx, k, width, height);
            }

            // Bounce unused
            _ => {
                return InputResult::Key(k);
            }
        }
        InputResult::Rerender
    }
}

impl Renderable for MainPanel {
    fn name(&self) -> String {
        "torrents".into()
    }
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        // If the display got downsized, we possibly need to tighten the torrent selection
        let d = self.torrents.1 - self.torrents.0;
        let sub = if self.details.1.is_empty() { 0 } else { 6 };
        // - 2 because of the server footer, -1 because of 1-0 index conversion
        let torr_height = height.saturating_sub(3 + sub) as usize;
        if d > torr_height {
            self.torrents.1 -= d - torr_height;
        }

        let draw_torrents = |target: &mut _, width: u16, height, x, y| {
            let mut width_status = 0;
            let mut width_throttle_up = 0;
            let mut width_throttle_down = 0;
            let mut width_ratio = 0;
            for t in self.torrents
                .2
                .iter()
                .skip(self.torrents.0)
                .take(height as _)
            {
                width_status = cmp::max(width_status, t.status.as_str().len());
                width_throttle_up = cmp::max(
                    width_throttle_up,
                    t.throttle_up
                        .map(|t| if t == -1 { 1 } else { 10 })
                        .unwrap_or(6),
                );
                width_throttle_down = cmp::max(
                    width_throttle_down,
                    t.throttle_down
                        .map(|t| if t == -1 { 1 } else { 10 })
                        .unwrap_or(6),
                );
                width_ratio = cmp::max(
                    width_ratio,
                    if t.transferred_down == 0 {
                        4
                    } else {
                        3 + {
                            let rat = t.transferred_up as f32 / t.transferred_down as f32;
                            if rat <= 1. {
                                1
                            } else {
                                (1. + rat.log10().floor()) as usize
                            }
                        }
                    },
                );
            }

            let width_right = (3 + 2 + width_status + 1 + 10 + 1 + width_throttle_up + 3 + 10 + 1
                + width_throttle_down + 5 + width_ratio + 2 + 10 + 3
                + 10 + 1) as u16;
            for (i, t) in self.torrents
                .2
                .iter()
                .skip(self.torrents.0)
                .take(height as _)
                .enumerate()
            {
                let (c_s, c_e) = match self.focus {
                    Focus::Torrents
                        if i + self.torrents.0 == self.torrents.1 && t.error.is_some() =>
                    {
                        (
                            format!("{}{}", color::Fg(color::Cyan), color::Bg(color::Red)),
                            format!("{}{}", color::Fg(color::Reset), color::Bg(color::Reset)),
                        )
                    }
                    Focus::Torrents if i + self.torrents.0 == self.torrents.1 => (
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
                ).render(
                    target,
                    width.saturating_sub(width_right + 1),
                    1,
                    x,
                    y + i as u16,
                );
                widgets::Text::<_, align::x::Right, align::y::Top>::new(
                    true,
                    format!(
                        "{}{:03}% {: ^w_status$} {: >10}[{: ^w_tu$}]↑ {: >10}[{: ^w_td$}]↓   \
                         {: >w_rat$.2}  {: >10}↑  {: >10}↓{}",
                        c_s,
                        (t.progress * 100.).round(),
                        t.status.as_str(),
                        t.rate_up.fmt_size(),
                        t.throttle_up
                            .map(|t| if t == -1 {
                                "∞".into()
                            } else {
                                t.fmt_size()
                            })
                            .unwrap_or_else(|| "global".into()),
                        t.rate_down.fmt_size(),
                        t.throttle_down
                            .map(|t| if t == -1 {
                                "∞".into()
                            } else {
                                t.fmt_size()
                            })
                            .unwrap_or_else(|| "global".into()),
                        if t.transferred_down == 0 {
                            1.
                        } else {
                            t.transferred_up as f32 / t.transferred_down as f32
                        },
                        t.transferred_up.fmt_size(),
                        t.transferred_down.fmt_size(),
                        c_e,
                        w_status = width_status,
                        w_tu = width_throttle_up,
                        w_td = width_throttle_down,
                        w_rat = width_ratio,
                    ),
                ).render(
                    target,
                    width_right,
                    1,
                    x + (width - width_right),
                    y + i as u16,
                );
            }
            if let Some(ref filter) = self.filter {
                widgets::Text::<_, align::x::Left, align::y::Top>::new(
                    true,
                    match self.focus {
                        Focus::Filter => filter.format(true),
                        _ => filter.format(false),
                    },
                ).render(target, width, 1, x, height);
            }
        };
        let draw_trackers = |target: &mut _, width, height, x, y| {
            let sel_tor = match self.focus {
                Focus::Torrents | Focus::Filter => self.torrents.2.get(self.torrents.1),
                Focus::Details => self.details.1.get(self.details.0).map(|t| t.inner()),
            };
            for (i, &(ref base, ref others)) in self.trackers.iter().take(height as _).enumerate() {
                let matches = sel_tor
                    .as_ref()
                    .map(|t| {
                        t.tracker_urls
                            .iter()
                            .any(|u| &*u == base.url.host_str().unwrap())
                    })
                    .unwrap_or(false);
                let (c_s, c_e) = match (
                    matches,
                    base.error.is_some() || others.iter().any(|&(_, ref id, ref e)| {
                        sel_tor.map(|t| t.id == *id).unwrap_or(false) && e.is_some()
                    }),
                ) {
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
                        "{}{} {}{}",
                        c_s,
                        others.len() + 1,
                        base.url.host_str().unwrap(),
                        c_e,
                    ),
                ).render(target, width, 1, x, y + i as u16);
            }
        };
        let draw_details = |target: &mut _, width, height, x, y| {
            // FIXME: The unsafe avoids clones; This is perfectly safe but not possible without
            // "Closures Capture Disjoint Fields" safely
            widgets::BorrowedSameTabs::new(
                unsafe {
                    ::std::slice::from_raw_parts_mut(
                        self.details.1.as_ptr() as *mut TorrentDetailsPanel,
                        self.details.1.len(),
                    )
                },
                self.details.0,
            ).render(target, width, height, x, y);
        };
        let draw_footer = |target: &mut _, width, height, x, y| {
            widgets::Text::<_, align::x::Left, align::y::Top>::new(
                true,
                format!(
                    "Server: {} {}, {}   {}[{}]↑ {}[{}]↓   \
                     Session: {:.2}, {}↑ {}↓   Lifetime: {:.2}, {}↑ {}↓",
                    self.server_version,
                    fmt::date_diff_now(self.server.started),
                    self.server.free_space.fmt_size(),
                    self.server.rate_up.fmt_size(),
                    self.server
                        .throttle_up
                        .map(|t| if t == -1 {
                            "∞".into()
                        } else {
                            t.fmt_size()
                        })
                        .unwrap_or_else(|| "∞".into()),
                    self.server.rate_down.fmt_size(),
                    self.server
                        .throttle_down
                        .map(|t| if t == -1 {
                            "∞".into()
                        } else {
                            t.fmt_size()
                        })
                        .unwrap_or_else(|| "∞".into()),
                    if self.server.ses_transferred_down == 0 {
                        1.
                    } else {
                        self.server.ses_transferred_up as f32
                            / self.server.ses_transferred_down as f32
                    },
                    self.server.ses_transferred_up.fmt_size(),
                    self.server.ses_transferred_down.fmt_size(),
                    if self.server.transferred_down == 0 {
                        1.
                    } else {
                        self.server.transferred_up as f32 / self.server.transferred_down as f32
                    },
                    self.server.transferred_up.fmt_size(),
                    self.server.transferred_down.fmt_size(),
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
    fn rpc(&mut self, _: &RpcContext, msg: SMessage) -> bool {
        match msg {
            SMessage::RpcVersion(ver) => {
                self.server_version = format!("{}.{}", ver.major, ver.minor);
                true
            }
            SMessage::ResourcesRemoved { ids, .. } => {
                // FIXME: Some shittiness can go once closure disjoint field borrows land
                let mut i = 0;
                let mut dec = 0;
                let idx = self.torrents.1;
                self.torrents.2.retain(|t| {
                    i += 1;
                    if ids.contains(&t.id) {
                        if i - 1 == idx && i != 1 {
                            dec += 1;
                        }
                        false
                    } else {
                        true
                    }
                });
                self.torrents.0.saturating_sub(dec);
                self.torrents.1.saturating_sub(dec);

                i = 0;
                dec = 0;
                let idx = self.details.0;
                self.details.1.retain(|t| {
                    i += 1;
                    if ids.contains(&t.inner().id) {
                        if i - 1 == idx && i != 1 {
                            dec += 1;
                        }
                        false
                    } else {
                        true
                    }
                });
                self.details.0.saturating_sub(dec);
                if self.details.1.is_empty() && self.focus == Focus::Details {
                    self.focus = Focus::Torrents;
                }

                // FIXME: Once drain_filter lands, use that
                let mut idx = 0;
                while idx < self.trackers.len() {
                    let mut rm = false;

                    {
                        let (ref mut base, ref mut others) = self.trackers[idx];

                        others.retain(|&(ref id, _, _)| !ids.contains(&id));

                        if ids.contains(&base.id) {
                            if others.is_empty() {
                                rm = true;
                            } else {
                                let last = others.pop().unwrap();
                                base.id = last.0;
                                base.torrent_id = last.1;
                                base.error = last.2;
                            }
                        }
                    }

                    if rm {
                        self.trackers.remove(idx);
                    } else {
                        idx += 1;
                    }
                }

                true
            }
            SMessage::UpdateResources { resources, .. } => {
                let mut name_cache = match resources.first() {
                    Some(&SResourceUpdate::Resource(Cow::Owned(Resource::Torrent(_))))
                    | Some(&SResourceUpdate::Resource(Cow::Borrowed(&Resource::Torrent(_)))) => {
                        Some(HashMap::with_capacity(resources.len()))
                    }
                    _ => None,
                };
                'UPDATES: for upd in resources {
                    match upd {
                        // New resource insertion
                        SResourceUpdate::Resource(res) => match res.into_owned() {
                            Resource::Server(s) => {
                                self.server = s;
                            }
                            Resource::Torrent(t) => {
                                let mut name = t.name
                                    .as_ref()
                                    .map(|n| n.to_lowercase())
                                    .unwrap_or_else(|| "".to_owned());
                                let idx = self.torrents
                                    .2
                                    .binary_search_by(|probe| {
                                        natord::compare(
                                            name_cache
                                                .as_mut()
                                                .unwrap()
                                                .entry(probe.id.clone())
                                                .or_insert_with(|| {
                                                    probe
                                                        .name
                                                        .as_ref()
                                                        .map(|n| n.to_lowercase())
                                                        .unwrap_or_else(|| "".to_owned())
                                                }),
                                            &name,
                                        )
                                    })
                                    .unwrap_or_else(|e| e);
                                name_cache.as_mut().unwrap().insert(t.id.clone(), name);
                                self.torrents.2.insert(idx, t);
                            }
                            Resource::Tracker(t) => {
                                let mut new_pos = self.trackers.len();
                                for (i, &mut (ref mut base, ref mut others)) in
                                    self.trackers.iter_mut().enumerate()
                                {
                                    match t.url.cmp(&base.url) {
                                        Ordering::Equal => {
                                            let idx = others
                                                .binary_search_by_key(&&t.id, |&(ref id, _, _)| id)
                                                .unwrap_or_else(|e| e);
                                            others.insert(idx, (t.id, t.torrent_id, t.error));
                                            continue 'UPDATES;
                                        }
                                        Ordering::Less => {
                                            new_pos = i;
                                            break;
                                        }
                                        _ => {}
                                    }
                                }
                                self.trackers.insert(new_pos, (t, Vec::new()));
                            }
                            // Ignore other resources for now
                            _ => (),
                        },
                        // Server updates
                        SResourceUpdate::Throttle {
                            kind: ResourceKind::Server,
                            throttle_up,
                            throttle_down,
                            ..
                        } => {
                            self.server.throttle_up = throttle_up;
                            self.server.throttle_down = throttle_down;
                        }
                        SResourceUpdate::Rate {
                            kind: ResourceKind::Server,
                            rate_up,
                            rate_down,
                            ..
                        } => {
                            self.server.rate_up = rate_up;
                            self.server.rate_down = rate_down;
                        }
                        SResourceUpdate::ServerTransfer {
                            rate_up,
                            rate_down,
                            transferred_up,
                            transferred_down,
                            ses_transferred_up,
                            ses_transferred_down,
                            ..
                        } => {
                            self.server.rate_up = rate_up;
                            self.server.rate_down = rate_down;
                            self.server.transferred_up = transferred_up;
                            self.server.transferred_down = transferred_down;
                            self.server.ses_transferred_up = ses_transferred_up;
                            self.server.ses_transferred_down = ses_transferred_down;
                        }
                        SResourceUpdate::ServerSpace { free_space, .. } => {
                            self.server.free_space = free_space;
                        }
                        SResourceUpdate::ServerToken { download_token, .. } => {
                            self.server.download_token = download_token;
                        }
                        // Tracker updates
                        SResourceUpdate::TrackerStatus {
                            id,
                            last_report,
                            error,
                            ..
                        } => for &mut (ref mut base, ref mut others) in &mut self.trackers {
                            if id == base.id {
                                base.last_report = last_report;
                                base.error = error;
                                break;
                            } else if let Ok(pos) =
                                others.binary_search_by_key(&&id, |&(ref id, _, _)| id)
                            {
                                base.last_report = last_report;
                                others[pos].2 = error;
                                break;
                            }
                        },
                        // Torrent updates
                        SResourceUpdate::Throttle {
                            kind: ResourceKind::Torrent,
                            ..
                        }
                        | SResourceUpdate::Rate {
                            kind: ResourceKind::Torrent,
                            ..
                        }
                        | SResourceUpdate::TorrentStatus { .. }
                        | SResourceUpdate::TorrentTransfer { .. }
                        | SResourceUpdate::TorrentPeers { .. }
                        | SResourceUpdate::TorrentPicker { .. }
                        | SResourceUpdate::TorrentPriority { .. }
                        | SResourceUpdate::TorrentPath { .. }
                        | SResourceUpdate::TorrentPieces { .. } => {
                            for t in self.details.1.iter_mut().map(|t| t.inner_mut()) {
                                if upd.id() == &*t.id {
                                    t.update(upd.clone());
                                    // The id will also be in the torrent list
                                    break;
                                }
                            }
                            for t in &mut self.torrents.2 {
                                if upd.id() == &*t.id {
                                    t.update(upd);
                                    break;
                                }
                            }
                        }
                        _ => (),
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
        ctx.send(CMessage::FilterSubscribe {
            serial: ctx.next_serial(),
            kind: ResourceKind::Tracker,
            criteria: Vec::new(),
        });
        unsafe {
            filter::init(ctx);
        }
    }
}

#[derive(Clone)]
struct TorrentDetailsPanel {
    torr: Torrent,
}
impl TorrentDetailsPanel {
    fn new(torr: Torrent) -> TorrentDetailsPanel {
        TorrentDetailsPanel { torr }
    }
    fn inner(&self) -> &Torrent {
        &self.torr
    }
    fn inner_mut(&mut self) -> &mut Torrent {
        &mut self.torr
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
                    fmt::date_diff_now(self.torr.created),
                    fmt::date_diff_now(self.torr.modified),
                ),
            ).render(target, width, 1, x_off, y_off);
        }

        if height >= 2 {
            widgets::Text::<_, align::x::Left, align::y::Top>::new(
                true,
                format!(
                    "Rates: {}[{}]↑ {}[{}]↓    Lifetime: {:.2} {}↑ {}↓",
                    self.torr.rate_up.fmt_size(),
                    self.torr
                        .throttle_up
                        .map(|t| if t == -1 {
                            "∞".into()
                        } else {
                            t.fmt_size()
                        })
                        .unwrap_or_else(|| "global".into()),
                    self.torr.rate_down.fmt_size(),
                    self.torr
                        .throttle_down
                        .map(|t| if t == -1 {
                            "∞".into()
                        } else {
                            t.fmt_size()
                        })
                        .unwrap_or_else(|| "global".into()),
                    if self.torr.transferred_down == 0 {
                        1.
                    } else {
                        self.torr.transferred_up as f32 / self.torr.transferred_down as f32
                    },
                    self.torr.transferred_up.fmt_size(),
                    self.torr.transferred_down.fmt_size(),
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
                        .map(|p| p.fmt_size())
                        .unwrap_or_else(|| "?".into()),
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
                        .unwrap_or_else(|| "?".into()),
                    self.torr
                        .pieces
                        .map(|p| format!("{}", p))
                        .unwrap_or_else(|| "?".into()),
                    self.torr
                        .piece_size
                        .map(|p| p.fmt_size())
                        .unwrap_or_else(|| "?".into()),
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
