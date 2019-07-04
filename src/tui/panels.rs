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
use synapse_rpc::{
    message::{CMessage, SMessage},
    resource::{Resource, ResourceKind, SResourceUpdate, Server, Torrent, Tracker},
};
use termion::event::Key;

use std::cmp::{self, Ordering};

use crate::{
    rpc,
    tui::{widgets, Component, HandleInput, HandleRpc, InputResult, Renderable},
    utils::{
        align,
        color::ColorEscape,
        filter::Filter,
        fmt::{self, FormatSize},
    },
};

mod login;
mod torrent_details;

pub use self::login::Login;
pub use self::torrent_details::TorrentDetails;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    Details,
    Filter,
    Torrents,
}

#[derive(Clone)]
pub struct Main {
    last_height: u16,
    focus: Focus,
    filter: Filter,
    filter_disp: bool,
    // FIXME: anon names
    // lower bound of torrent selection,  current pos, _
    torrents: (usize, usize, Vec<Torrent>),
    // status, throttle up/down, ratio, right
    torrent_widths: (usize, usize, usize, usize, usize),
    // tracker base, Vec<(tracker id, torrent_id, optional error)>
    trackers: Vec<(Tracker, Vec<(String, String, Option<String>)>)>,
    trackers_disp: bool,
    details: (usize, Vec<TorrentDetails>),
    server: Server,
    server_version: String,
}

impl Main {
    pub fn new(sink: &rpc::WsSink, height: u16) -> Main {
        rpc::send(
            sink,
            CMessage::FilterSubscribe {
                serial: rpc::next_serial(),
                kind: ResourceKind::Server,
                criteria: Vec::new(),
            },
        );
        rpc::send(
            sink,
            CMessage::FilterSubscribe {
                serial: rpc::next_serial(),
                kind: ResourceKind::Tracker,
                criteria: Vec::new(),
            },
        );

        Main {
            last_height: height,
            focus: Focus::Torrents,
            filter: Filter::new(sink),
            filter_disp: false,
            torrents: (0, 0, Vec::new()),
            torrent_widths: (0, 0, 0, 0, 0),
            trackers: Vec::new(),
            trackers_disp: false,
            details: (0, Vec::new()),
            server: Default::default(),
            server_version: "?.?".to_owned(),
        }
    }

    fn recompute_torrent_bounds(&mut self, height: u16) {
        self.torrent_widths.0 = 0;
        self.torrent_widths.1 = 0;
        self.torrent_widths.2 = 0;
        self.torrent_widths.3 = 0;
        for t in self
            .torrents
            .2
            .iter()
            .skip(self.torrents.0)
            .take(height as _)
        {
            self.torrent_widths.0 = cmp::max(self.torrent_widths.0, t.status.as_str().len());
            self.torrent_widths.1 = cmp::max(
                self.torrent_widths.1,
                t.throttle_up
                    .map(|t| if t == -1 { 1 } else { 10 })
                    .unwrap_or(1),
            );
            self.torrent_widths.2 = cmp::max(
                self.torrent_widths.2,
                t.throttle_down
                    .map(|t| if t == -1 { 1 } else { 10 })
                    .unwrap_or(1),
            );
            self.torrent_widths.3 = cmp::max(
                self.torrent_widths.3,
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

        self.torrent_widths.4 = 62
            + self.torrent_widths.0
            + self.torrent_widths.1
            + self.torrent_widths.2
            + self.torrent_widths.3;
    }
}

impl Component for Main {}

impl HandleInput for Main {
    fn input(&mut self, k: Key, width: u16, height: u16) -> InputResult {
        // - 2 because of the server footer
        let torr_height = height.saturating_sub(2) as usize;
        let torr_list_height = if self.details.1.is_empty() {
            (torr_height as u16).saturating_sub(2)
        } else {
            (torr_height as u16).saturating_sub(2 + 5)
        };

        match (k, self.focus) {
            // Special keys
            (Key::Ctrl('f'), Focus::Filter) => {
                self.focus = Focus::Torrents;
                self.filter.reset();
                self.filter_disp = false;
                self.recompute_torrent_bounds(torr_list_height);
            }

            (Key::Esc, Focus::Filter) => {
                self.focus = Focus::Torrents;
            }

            // Movement Keys
            (Key::Home, Focus::Torrents) => {
                self.torrents.0 = 0;
                self.torrents.1 = 0;
                self.recompute_torrent_bounds(torr_list_height);
            }
            (Key::Home, Focus::Details) => {
                self.details.0 = 0;
            }

            (Key::End, Focus::Torrents) => {
                let l = self.torrents.2.len();
                self.torrents.0 = l.saturating_sub(torr_height);
                self.torrents.1 = l.saturating_sub(1);
                self.recompute_torrent_bounds(torr_list_height);
            }
            (Key::End, Focus::Details) => {
                self.details.0 = self.details.1.len() - 1;
            }

            (Key::PageUp, Focus::Torrents) if self.torrents.1 < torr_height => {
                self.torrents.0 = 0;
                self.torrents.1 = 0;
                self.recompute_torrent_bounds(torr_list_height);
            }
            (Key::PageUp, Focus::Torrents) => {
                if self.torrents.0 < torr_height {
                    self.torrents.0 = 0;
                } else {
                    self.torrents.0 -= torr_height;
                }
                self.torrents.1 -= torr_height;
                self.recompute_torrent_bounds(torr_list_height);
            }

            (Key::PageDown, Focus::Torrents)
                if self.torrents.1 + torr_height >= self.torrents.2.len() =>
            {
                let l = self.torrents.2.len();
                self.torrents.0 = l.saturating_sub(torr_height);
                self.torrents.1 = l.saturating_sub(1);
                self.recompute_torrent_bounds(torr_list_height);
            }
            (Key::PageDown, Focus::Torrents) => {
                if self.torrents.0 + 2 * torr_height >= self.torrents.2.len() {
                    self.torrents.0 = self.torrents.2.len().saturating_sub(torr_height);
                } else {
                    self.torrents.0 += torr_height;
                }
                self.torrents.1 += torr_height;
                self.recompute_torrent_bounds(torr_list_height);
            }

            (Key::Up, Focus::Torrents) | (Key::Char('k'), Focus::Torrents)
                if self.torrents.1 > 0 =>
            {
                if self.torrents.0 == self.torrents.1 {
                    self.torrents.0 -= 1;
                }
                self.torrents.1 -= 1;
                self.recompute_torrent_bounds(torr_list_height);
            }

            (Key::Down, Focus::Torrents) | (Key::Char('j'), Focus::Torrents)
                if self.torrents.1 + 1 < self.torrents.2.len() =>
            {
                if self.torrents.0 + torr_height.saturating_sub(1) == self.torrents.1 {
                    self.torrents.0 += 1;
                }
                self.torrents.1 += 1;
                self.recompute_torrent_bounds(torr_list_height);
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
            //(Key::Char('\n'), Focus::Torrents) => unimplemented!("OPEN DIR"),
            (Key::Char('d'), Focus::Torrents) if !self.torrents.2.is_empty() => {
                if let Some(pos) = self
                    .details
                    .1
                    .iter()
                    .position(|dt| dt.inner().id == self.torrents.2[self.torrents.1].id)
                {
                    self.details.0 = pos;
                } else {
                    self.details.1.push(TorrentDetails::new(
                        self.torrents.2[self.torrents.1].clone(),
                    ));
                    self.details.0 = self.details.1.len() - 1;
                }
                self.focus = Focus::Details;
                self.recompute_torrent_bounds(torr_list_height.saturating_sub(5));
            }

            (Key::Char('e'), Focus::Torrents) | (Key::Char('e'), Focus::Details) => {
                return if self.focus == Focus::Torrents {
                    self.torrents.2.get(self.torrents.1)
                } else {
                    self.details.1.get(self.details.0).map(|d| d.inner())
                }
                .and_then(|t| {
                    let mut tree = Vec::new();
                    let mut len = "Errors".len() as u16;
                    if let Some(ref e) = t.error {
                        tree.push(e.clone());
                        len = cmp::max(len, e.len() as u16);
                    };
                    for &(ref base, ref others) in self.trackers.iter().filter(|tra| {
                        t.tracker_urls
                            .iter()
                            .any(|tu| *tu == tra.0.url.host_str().unwrap())
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

                    let tlen = tree.len() as _;
                    // FIXME: Cloning self here is pretty hacky
                    Some(InputResult::ReplaceWith(
                        Box::new(widgets::OwnedOverlay::new(
                            widgets::CloseOnInput::new(
                                widgets::IgnoreRpc::new(widgets::RenderStateFn::new(draw, tree)),
                                &[],
                            ),
                            Box::new(self.clone()),
                            (len, tlen),
                            Some(ColorEscape::red()),
                            "Errors".to_owned(),
                        )) as Box<Component>,
                    ))
                })
                .unwrap_or(InputResult::Key(Key::Char('e')));
            }

            (Key::Char('f'), Focus::Torrents) | (Key::Char('f'), Focus::Details) => {
                self.focus = Focus::Filter;
                if !self.filter_disp {
                    self.filter_disp = true;
                }
            }

            (Key::Char('J'), Focus::Torrents) if !self.details.1.is_empty() => {
                self.focus = Focus::Details;
            }

            (Key::Char('K'), Focus::Details) => {
                self.focus = Focus::Torrents;
            }

            (Key::Char('q'), Focus::Details) => {
                // This is ok, because details only focused when not empty
                self.details.1.remove(self.details.0);
                if self.details.1.is_empty() {
                    self.focus = Focus::Torrents;
                    self.recompute_torrent_bounds(torr_list_height.saturating_sub(5));
                } else if self.details.0 == self.details.1.len() {
                    self.details.0 -= 1;
                }
            }

            (Key::Char('t'), Focus::Torrents) | (Key::Char('t'), Focus::Details) => {
                self.trackers_disp = !self.trackers_disp;
                self.recompute_torrent_bounds(torr_list_height.saturating_sub(5));
            }

            // Catch all filter input
            (k, Focus::Filter) => {
                return self.filter.input(k, width, height);
            }

            // Bounce unused
            _ => {
                return InputResult::Key(k);
            }
        }
        InputResult::Rerender
    }
}

impl Renderable for Main {
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        // If the window got downsized, we need to tighten the torrent selection
        let d = self.torrents.1 - self.torrents.0;
        let sub = if self.details.1.is_empty() { 0 } else { 6 };
        // - 2 because of the server footer, -1 because of 1-0 index conversion
        let torr_height = height.saturating_sub(3 + sub) as usize;
        if d > torr_height {
            self.torrents.1 -= d - torr_height;
        }
        if height != self.last_height {
            self.last_height = height;
            self.recompute_torrent_bounds(height);
        }

        let draw_torrents = |target: &mut _, width: u16, height, x, y| {
            for (i, t) in self
                .torrents
                .2
                .iter()
                .skip(self.torrents.0)
                .take(height as _)
                .enumerate()
            {
                let tracker_err = self
                    .trackers
                    .iter()
                    .filter(|tra| {
                        t.tracker_urls
                            .iter()
                            .any(|tu| *tu == tra.0.url.host_str().unwrap())
                    })
                    .any(|(ref base, ref others)| {
                        base.error.is_some()
                            || others
                                .iter()
                                .any(|&(_, ref id, ref e)| t.id == *id && e.is_some())
                    });

                let (c_s, c_e) = match self.focus {
                    Focus::Torrents
                        if i + self.torrents.0 == self.torrents.1
                            && (t.error.is_some() || tracker_err) =>
                    {
                        (
                            format!("{}{}", ColorEscape::cyan(), ColorEscape::red_bg()),
                            format!("{}{}", ColorEscape::reset(), ColorEscape::reset_bg()),
                        )
                    }
                    Focus::Torrents if i + self.torrents.0 == self.torrents.1 => (
                        format!("{}", ColorEscape::cyan()),
                        format!("{}", ColorEscape::reset()),
                    ),
                    _ if t.error.is_some() || tracker_err => (
                        format!("{}", ColorEscape::red()),
                        format!("{}", ColorEscape::reset()),
                    ),
                    _ => ("".into(), "".into()),
                };

                let (render_stats, width_left) =
                    if width.saturating_sub(self.torrent_widths.4 as u16 + 1) < width / 3 {
                        (false, width)
                    } else {
                        (true, width.saturating_sub(self.torrent_widths.4 as u16 + 1))
                    };

                widgets::Text::<_, align::x::Left, align::y::Top>::new(
                    true,
                    format!(
                        "{}{}{}",
                        c_s,
                        &**t.name.as_ref().unwrap_or_else(|| &t.path),
                        c_e
                    ),
                )
                .render(target, width_left, 1, x, y + i as u16);
                if render_stats {
                    widgets::Text::<_, align::x::Right, align::y::Top>::new(
                        true,
                        format!(
                            "{}{: >3}% {: ^w_status$} {}[{: ^w_tu$}]↑ {}[{: ^w_td$}]↓   \
                             {: >w_rat$.2}  {}↑  {}↓{}",
                            c_s,
                            (t.progress * 100.).round(),
                            t.status.as_str(),
                            t.rate_up.fmt_size_align(),
                            t.throttle_up
                                .map(|t| if t == -1 {
                                    "∞".into()
                                } else {
                                    t.fmt_size_align()
                                })
                                .unwrap_or_else(|| "*".into()),
                            t.rate_down.fmt_size(),
                            t.throttle_down
                                .map(|t| if t == -1 {
                                    "∞".into()
                                } else {
                                    t.fmt_size_align()
                                })
                                .unwrap_or_else(|| "*".into()),
                            if t.transferred_down == 0 {
                                0.
                            } else {
                                t.transferred_up as f32 / t.transferred_down as f32
                            },
                            t.transferred_up.fmt_size_align(),
                            t.transferred_down.fmt_size_align(),
                            c_e,
                            w_status = self.torrent_widths.0,
                            w_tu = self.torrent_widths.1,
                            w_td = self.torrent_widths.2,
                            w_rat = self.torrent_widths.3,
                        ),
                    )
                    .render(
                        target,
                        cmp::min(self.torrent_widths.4 as u16, width),
                        1,
                        x + width.saturating_sub(self.torrent_widths.4 as u16),
                        y + i as u16,
                    );
                }
            }
            if self.filter_disp {
                widgets::Text::<_, align::x::Left, align::y::Top>::new(
                    true,
                    match self.focus {
                        Focus::Filter => self.filter.format(true),
                        _ => self.filter.format(false),
                    },
                )
                .render(target, width, 1, x, height);
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
                            .any(|u| *u == base.url.host_str().unwrap())
                    })
                    .unwrap_or(false);
                let (c_s, c_e) = match (
                    matches,
                    (base.error.is_some()
                        && base.torrent_id == sel_tor.map(|t| &*t.id).unwrap_or(""))
                        || others.iter().any(|&(_, ref id, ref e)| {
                            sel_tor.map(|t| t.id == *id).unwrap_or(false) && e.is_some()
                        }),
                ) {
                    (true, true) => (
                        format!("{}{}", ColorEscape::cyan(), ColorEscape::red_bg()),
                        format!("{}{}", ColorEscape::reset(), ColorEscape::reset_bg()),
                    ),
                    (true, false) => (
                        format!("{}", ColorEscape::cyan()),
                        format!("{}", ColorEscape::reset()),
                    ),
                    (false, true) => (
                        format!("{}", ColorEscape::red()),
                        format!("{}", ColorEscape::reset()),
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
                )
                .render(target, width, 1, x, y + i as u16);
            }
        };
        let draw_details = |target: &mut _, width, height, x, y| {
            // FIXME: The unsafe avoids clones; This is perfectly safe but not possible without
            // "Closures Capture Disjoint Fields" in safe rust afaict
            widgets::BorrowedSameTabs::new(
                unsafe {
                    ::std::slice::from_raw_parts_mut(
                        self.details.1.as_ptr() as *mut TorrentDetails,
                        self.details.1.len(),
                    )
                },
                self.details.0,
            )
            .render(target, width, height, x, y);
        };
        let draw_footer = |target: &mut _, width, height, x, y| {
            widgets::Text::<_, align::x::Left, align::y::Top>::new(
                true,
                format!(
                    "Server {}: {} {}↑,   {}[{}]↑ {}[{}]↓,   \
                     Session: {}↑ {}↓ → {:.2},   Lifetime: {}↑ {}↓ → {:.2}",
                    self.server_version,
                    self.server.free_space.fmt_size(),
                    fmt::date_diff_now(self.server.started),
                    self.server.rate_up.fmt_size(),
                    self.server
                        .throttle_up
                        .map(|t| if t == -1 { "∞".into() } else { t.fmt_size() })
                        .unwrap_or_else(|| "∞".into()),
                    self.server.rate_down.fmt_size(),
                    self.server
                        .throttle_down
                        .map(|t| if t == -1 { "∞".into() } else { t.fmt_size() })
                        .unwrap_or_else(|| "∞".into()),
                    self.server.ses_transferred_up.fmt_size(),
                    self.server.ses_transferred_down.fmt_size(),
                    if self.server.ses_transferred_down == 0 {
                        1.
                    } else {
                        self.server.ses_transferred_up as f32
                            / self.server.ses_transferred_down as f32
                    },
                    self.server.transferred_up.fmt_size(),
                    self.server.transferred_down.fmt_size(),
                    if self.server.transferred_down == 0 {
                        1.
                    } else {
                        self.server.transferred_up as f32 / self.server.transferred_down as f32
                    },
                ),
            )
            .render(target, width, height, x, y);
        };

        match (self.trackers_disp, self.details.1.is_empty()) {
            (false, true) => {
                widgets::HSplit::new(
                    &mut widgets::RenderFn::new(draw_torrents) as &mut Renderable,
                    &mut widgets::RenderFn::new(draw_footer) as &mut Renderable,
                    None,
                    widgets::Unit::Lines(height.saturating_sub(2)),
                    true,
                )
                .render(target, width, height, x_off, y_off);
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
                )
                .render(target, width, height, x_off, y_off);
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
                )
                .render(target, width, height, x_off, y_off);
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
                )
                .render(target, width, height, x_off, y_off);
            }
        }
    }
}

impl HandleRpc for Main {
    fn rpc(&mut self, msg: SMessage) -> bool {
        match msg {
            SMessage::RpcVersion(ver) => {
                self.server_version = format!("{}.{}", ver.major, ver.minor);
                true
            }
            SMessage::ResourcesRemoved { ids, .. } => {
                // Remove matching resources, and move selection up/left if a resource up/left was
                // removed
                let mut i = 0;
                let mut dec = 0;
                let mut recomp_bounds = false;
                // TODO: With Closure disjoint borrows, these could be moved inside the closure
                let sel = self.torrents.1;
                let lower = self.torrents.0;
                let height = self.last_height;
                self.torrents.2.retain(|t| {
                    i += 1;
                    if ids.contains(&t.id) {
                        // The torrents are sorted, so we need to adjust the selection if it's above
                        // in the list
                        if i <= sel - dec && sel - dec != 0 {
                            dec += 1;
                            recomp_bounds = true;
                        } else if i >= lower && i - lower <= height as usize {
                            recomp_bounds = true;
                        }
                        false
                    } else {
                        true
                    }
                });
                self.torrents.0 -= dec;
                self.torrents.1 -= dec;

                if recomp_bounds {
                    self.recompute_torrent_bounds(height);
                }

                i = 0;
                dec = 0;
                let sel = self.details.0;
                self.details.1.retain(|t| {
                    i += 1;
                    if ids.contains(&t.inner().id) {
                        if i <= sel && sel - dec != 0 {
                            dec += 1;
                        }
                        false
                    } else {
                        true
                    }
                });
                self.details.0 -= dec;
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
                let mut recomp_bounds = false;
                'UPDATES: for upd in resources.into_iter() {
                    match upd {
                        // New resource insertion
                        SResourceUpdate::Resource(res) => match res.into_owned() {
                            Resource::Server(s) => {
                                self.server = s;
                            }
                            Resource::Torrent(t) => {
                                let empty = String::new();
                                let idx = self
                                    .torrents
                                    .2
                                    .binary_search_by(|probe| {
                                        natord::compare_ignore_case(
                                            probe.name.as_ref().unwrap_or_else(|| &empty),
                                            t.name.as_ref().unwrap_or_else(|| &empty),
                                        )
                                    })
                                    .unwrap_or_else(|e| e);

                                if idx >= self.torrents.0
                                    && idx - self.torrents.0 <= self.last_height as usize
                                {
                                    recomp_bounds = true;
                                }

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
                            ..
                        }
                        | SResourceUpdate::Rate {
                            kind: ResourceKind::Server,
                            ..
                        }
                        | SResourceUpdate::ServerTransfer { .. }
                        | SResourceUpdate::ServerSpace { .. }
                        | SResourceUpdate::ServerToken { .. } => {
                            self.server.update(upd);
                        }
                        // Tracker updates
                        SResourceUpdate::TrackerStatus {
                            id,
                            last_report,
                            error,
                            ..
                        } => {
                            for &mut (ref mut base, ref mut others) in &mut self.trackers {
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
                            }
                        }
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

                if recomp_bounds {
                    let h = self.last_height;
                    self.recompute_torrent_bounds(h);
                }
                true
            }
            _ => false,
        }
    }
}
