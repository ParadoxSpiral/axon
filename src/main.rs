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

extern crate chrono;
extern crate futures;
#[macro_use]
extern crate lazy_static;
extern crate natord;
extern crate parking_lot;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate shellexpand;
extern crate synapse_rpc;
extern crate termion;
extern crate tokio;
extern crate tokio_tungstenite as ws;
extern crate toml;
extern crate unicode_segmentation;
extern crate unicode_width;
extern crate url;

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

use tokio::prelude::*;
use tokio::runtime::{Runtime, TaskExecutor};

use config::CONFIG;
use tui::view::View;

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

// This is a bit hacky
static mut INTERNAL_EXECUTOR: Option<TaskExecutor> = None;
lazy_static! {
    static ref EXECUTOR: &'static TaskExecutor = unsafe { INTERNAL_EXECUTOR.as_ref().unwrap() };
    static ref VIEW: View = View::new();
}

fn main() {
    let rt = Runtime::new().unwrap();
    unsafe {
        INTERNAL_EXECUTOR = Some(rt.executor());
    }

    if CONFIG.autoconnect {
        #[cfg(feature = "dbg")]
        info!(*S_VIEW, "Autoconnecting");
        rpc::start_connect(
            &*CONFIG.server.clone().unwrap(),
            CONFIG
                .pass
                .clone()
                .as_ref()
                .map(|p| &**p)
                .unwrap_or_else(|| ""),
        );
    }

    input::start();

    rt.shutdown_on_idle().wait().unwrap();

    VIEW.restore_terminal();
}
