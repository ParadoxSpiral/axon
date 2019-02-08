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

mod config;
mod input;
mod rpc;
mod tui;
mod utils;

use futures::sync::mpsc;
use log::{info, warn};

use crate::{config::CONFIG, tui::view};

fn main() {
    env_logger::init();
    warn!("Do not share this log publicly without first removing sensitive information: Any address connected to, any decoded key presses while entering password or other sensitive information!\n\n");

    let (mut urls_s, urls_r) = mpsc::channel(1);;
    let conns = rpc::connections(urls_r);

    if CONFIG.autoconnect {
        info!("Autoconnecting");
        urls_s
            .try_send((
                CONFIG.server.clone().unwrap(),
                CONFIG.pass.clone().unwrap_or_default(),
            ))
            .unwrap();
    }

    tokio::run(view::run(urls_s, conns));
}
