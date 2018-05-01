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
extern crate crossbeam;
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
// TODO: Once the websockets impl uses the new tokio, we can drop this
extern crate tokio_core;
extern crate toml;
extern crate unicode_segmentation;
extern crate unicode_width;
extern crate url;
extern crate websocket;

#[cfg(feature = "dbg")]
#[cfg_attr(feature = "dbg", macro_use)]
extern crate slog;
#[cfg(feature = "dbg")]
extern crate slog_async;
#[cfg(feature = "dbg")]
extern crate slog_term;

pub mod config;
mod rpc;
mod tui;
pub mod utils;

use termion::event::Key;
use termion::input::TermRead;

use config::CONFIG;
use rpc::RpcContext;
use tui::InputResult;
use tui::View;

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
    static ref S_IO: slog::Logger = (*SLOG_ROOT).new(o!("IO" => true));
    static ref S_DEADLOCK: slog::Logger = (*SLOG_ROOT).new(o!("DEADLOCK" => true));
}

fn main() {
    let view = View::init();
    let rpc = RpcContext::new(&view);

    #[cfg(feature = "dbg")]
    {
        use parking_lot::deadlock;
        use std::thread;
        use std::time::Duration;

        // Create a background thread which checks for deadlocks every 10s
        thread::spawn(move || loop {
            thread::sleep(Duration::from_secs(10));
            let deadlocks = deadlock::check_deadlock();
            if deadlocks.is_empty() {
                continue;
            }

            let mut s = String::new();
            s.push_str(&*format!("{} deadlocks:", deadlocks.len()));
            for (i, threads) in deadlocks.iter().enumerate() {
                s.push_str(&*format!("\n\t{}:\n\n", i));
                for t in threads {
                    s.push_str(&*format!("\tThread: {:#?}", t.thread_id()));
                    s.push_str(&*format!("\n\t{:#?}", t.backtrace()));
                }
            }
            crit!(*S_DEADLOCK, "{}", s);
        });
    }

    crossbeam::scope(|scope| {
        if CONFIG.autoconnect {
            scope.spawn(|| {
                print!("Autoconnecting…");
                #[cfg(feature = "dbg")]
                trace!(*S_VIEW, "Autoconnecting");
                view.handle_input(&rpc, Key::Char('\n'));
            });
        }

        // View worker
        scope.spawn(|| {
            #[cfg(feature = "dbg")]
            trace!(*S_VIEW, "Entering loop");
            view.render_until_death();
        });

        // rpc worker
        scope.spawn(|| {
            #[cfg(feature = "dbg")]
            trace!(*S_RPC, "Entering loop");
            rpc.recv_until_death();
        });

        // Input worker
        scope.spawn(|| {
            let stdin = ::std::io::stdin();
            #[cfg(feature = "dbg")]
            trace!(*S_IO, "Entering loop");
            for ev in stdin.lock().keys() {
                let res = if let Ok(k) = ev {
                    // Pass input through components
                    #[cfg(feature = "dbg")]
                    {
                        if view.logged_in() {
                            debug!(*S_IO, "Handling {:?}", k);
                        } else {
                            debug!(*S_IO, "Handling key in LoginPanel");
                        }
                    }

                    view.handle_input(&rpc, k)
                } else {
                    #[cfg(feature = "dbg")]
                    crit!(*S_IO, "Fatal error: {}", ev.as_ref().unwrap_err());

                    if view.logged_in() {
                        rpc.disconnect();
                    }
                    rpc.disconnect();
                    view.shutdown();

                    panic!("Unrecoverable error: {:?}", ev.unwrap_err())
                };

                match res {
                    InputResult::Close => {
                        break;
                    }
                    InputResult::Rerender => {
                        view.wake();
                    }
                    InputResult::Key(_) => {}
                    _ => unreachable!(),
                }
            }
        });
    });
}
