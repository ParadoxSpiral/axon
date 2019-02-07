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

use synapse_rpc::resource::Torrent;

use crate::{
    tui::{widgets, Renderable},
    utils::{
        align,
        fmt::{self, FormatSize},
    },
};

#[derive(Clone)]
pub struct TorrentDetails {
    torr: Torrent,
}

impl TorrentDetails {
    pub fn new(torr: Torrent) -> TorrentDetails {
        TorrentDetails { torr }
    }
    pub fn inner(&self) -> &Torrent {
        &self.torr
    }
    pub fn inner_mut(&mut self) -> &mut Torrent {
        &mut self.torr
    }
}

impl Renderable for TorrentDetails {
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
                    "{}, {}    Picker: {:?}    Created: {} ago    Modified: {} ago",
                    if self.torr.private {
                        "Private"
                    } else {
                        "Public"
                    },
                    self.torr.status.as_str(),
                    self.torr.strategy,
                    fmt::date_diff_now(self.torr.created),
                    fmt::date_diff_now(self.torr.modified),
                ),
            )
            .render(target, width, 1, x_off, y_off);
        }

        if height >= 2 {
            widgets::Text::<_, align::x::Left, align::y::Top>::new(
                true,
                format!(
                    "Rates: {}[{}]↑ {}[{}]↓    Lifetime: {:.2} {}↑ {}↓",
                    self.torr.rate_up.fmt_size(),
                    self.torr
                        .throttle_up
                        .map(|t| if t == -1 { "∞".into() } else { t.fmt_size() })
                        .unwrap_or_else(|| "*".into()),
                    self.torr.rate_down.fmt_size(),
                    self.torr
                        .throttle_down
                        .map(|t| if t == -1 { "∞".into() } else { t.fmt_size() })
                        .unwrap_or_else(|| "*".into()),
                    if self.torr.transferred_down == 0 {
                        1.
                    } else {
                        self.torr.transferred_up as f32 / self.torr.transferred_down as f32
                    },
                    self.torr.transferred_up.fmt_size(),
                    self.torr.transferred_down.fmt_size(),
                ),
            )
            .render(target, width, 1, x_off, y_off + 1);
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
            )
            .render(target, width, 1, x_off, y_off + 2);
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
            )
            .render(target, width, 1, x_off, y_off + 3);
        }

        if height >= 5 {
            widgets::Text::<_, align::x::Left, align::y::Top>::new(
                true,
                format!("Path: {}", self.torr.path,),
            )
            .render(target, width, 1, x_off, y_off + 4);
        }
    }
}
