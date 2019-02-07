// Copyright (C) 2017  ParadoxSpiral
//
// This file is part of axon.
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

extern crate tokio_tungstenite as ws;

#[cfg(feature = "dbg")]
#[cfg_attr(feature = "dbg", macro_use)]
extern crate slog;
#[cfg(feature = "dbg")]
extern crate slog_async;
#[cfg(feature = "dbg")]
extern crate slog_term;

mod config;
mod input;
mod rpc;
mod tui;
mod utils;

use futures::sync::mpsc;
#[cfg(feature = "dbg")]
use lazy_static::lazy_static;

use crate::{config::CONFIG, tui::view};

#[cfg(feature = "dbg")]
lazy_static! {
    static ref SLOG_ROOT: slog::Logger = {
        use slog::Drain;
        use std::fs::OpenOptions;

        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open("debug_log")
            .unwrap();

        let decorator = slog_term::PlainDecorator::new(file);
        let drain = slog_term::CompactFormat::new(decorator).build().fuse();
        let drain = slog_async::Async::new(drain).build().fuse();

        ::slog::Logger::root(drain, o!("version" => env!("CARGO_PKG_VERSION")))
    };
    static ref S_RPC: slog::Logger = (*SLOG_ROOT).new(o!(
        "RPC version" => format!("{}.{}", synapse_rpc::MAJOR_VERSION, synapse_rpc::MINOR_VERSION)));
    static ref S_VIEW: slog::Logger = (*SLOG_ROOT).new(o!("View" => true));
}

fn main() {
    let (mut urls_s, urls_r) = mpsc::channel(1);;
    let conns = rpc::connections(urls_r);

    if CONFIG.autoconnect {
        #[cfg(feature = "dbg")]
        info!(*S_VIEW, "Autoconnecting");
        urls_s
            .try_send((
                CONFIG.server.clone().unwrap(),
                CONFIG.pass.clone().unwrap_or_default(),
            ))
            .unwrap();
    }

    tokio::run(view::run(urls_s, conns));
}
