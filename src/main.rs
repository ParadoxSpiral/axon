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
extern crate humansize;
extern crate itertools;
#[macro_use]
extern crate lazy_static;
extern crate parking_lot;
extern crate serde;
extern crate serde_json;
extern crate synapse_rpc;
extern crate termion;
extern crate tokio_core as tokio;
extern crate unicode_segmentation;
extern crate unicode_width;
extern crate url;
extern crate websocket;

mod rpc;
pub mod utils;
mod view;

use termion::input::TermRead;

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};

use rpc::RpcContext;
use view::View;
use view::tui::InputResult;

lazy_static!(
    static ref RUNNING: AtomicBool = AtomicBool::new(true);
);

fn main() {
    let view = View::init();
    let rpc = RpcContext::new(&view);

    crossbeam::scope(|scope| {
        // View worker
        scope.spawn(|| {
            view.render_until_death();
        });

        // rpc worker
        scope.spawn(|| {
            rpc.recv_until_death();
        });

        // Input worker
        scope.spawn(|| {
            let stdin = io::stdin();
            for ev in stdin.lock().keys() {
                let res = if let Ok(k) = ev {
                    // Pass input through components
                    view.handle_input(&rpc, k)
                } else {
                    RUNNING.store(false, Ordering::Release);
                    rpc.wake();
                    view.wake();
                    panic!("Unrecoverable error: {:?}", ev.unwrap_err())
                };

                match res {
                    InputResult::Close => {
                        RUNNING.store(false, Ordering::Release);
                        rpc.wake();
                        view.wake();
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
