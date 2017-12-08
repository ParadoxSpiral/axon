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

use libc;
use parking_lot::{Condvar, Mutex};
use url::Url;
use serde_json;
use synapse_rpc;
use websocket::ClientBuilder;
use websocket::client::sync::Client;
use websocket::message::OwnedMessage;
use websocket::result::WebSocketError;
use websocket::stream::sync::NetworkStream;

use std::cell::RefCell;
use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use view::View;

pub struct RpcContext<'a> {
    socket: RefCell<Option<Mutex<Client<Box<NetworkStream + Send>>>>>,
    waiter: (&'a Condvar, Mutex<()>),
}

unsafe impl<'a> Send for RpcContext<'a> {}
unsafe impl<'a> Sync for RpcContext<'a> {}

impl<'a> RpcContext<'a> {
    pub fn empty(cnd: &'a Condvar) -> RpcContext<'a> {
        RpcContext {
            socket: RefCell::new(None),
            waiter: (cnd, Mutex::new(())),
        }
    }

    pub fn init(&self, mut srv: Url, pass: &str) -> Result<(), String> {
        let url = srv.query_pairs_mut().append_pair("password", pass).finish();
        // FIXME: can't specify timeout -> connect to ws://1.1.1.1 -> 2.5m wait time until timeout
        let mut client = ClientBuilder::new(url.as_str())
            .map_err(|err| format!("{}", err))?
            .connect(None)
            .map_err(|err| format!("{:?}", err))?;

        let msg = client.recv_message();
        if let Ok(OwnedMessage::Text(msg)) = msg {
            let srv_ver = serde_json::from_str::<synapse_rpc::message::Version>(&msg)
                .map_err(|err| format!("{:?}", err))?;
            if srv_ver.major != synapse_rpc::MAJOR_VERSION {
                return Err(format!(
                    "Server version {:?} incompatible with client {}.{}",
                    srv_ver,
                    synapse_rpc::MAJOR_VERSION,
                    synapse_rpc::MINOR_VERSION
                ));
            }
        } else {
            return Err(format!("Expected server version, got {:?}", msg));
        }

        (**client.stream_ref())
            .as_tcp()
            .set_nonblocking(true)
            .map_err(|err| format!("{:?}", err))?;

        *self.socket.borrow_mut() = Some(Mutex::new(client));
        self.waiter.0.notify_one();
        Ok(())
    }

    pub fn recv_until_death(&self, running: &AtomicBool, view: &View) {
        self.waiter.0.wait(&mut self.waiter.1.lock());
        // Check if exited before login
        let socket = self.socket.borrow();
        if socket.is_none() {
            return;
        }
        let socket = socket.as_ref().unwrap();
        'OUTER: while running.load(Ordering::Acquire) {
            let mut ws = socket.lock();
            loop {
                match ws.recv_message() {
                    Ok(OwnedMessage::Ping(p)) => {
                        if let Err(err) = ws.send_message(&OwnedMessage::Pong(p)) {
                            view.global_err(format!("{}", err.description()));
                        };
                    }
                    Ok(OwnedMessage::Text(s)) => {
                        let s: Result<::synapse_rpc::message::SMessage, _> =
                            serde_json::from_str(&s);
                        if let Err(err) = s {
                            view.global_err(format!("{}", err.description()));
                        } else {
                            drop(ws);
                            view.handle_rpc(self, &s.unwrap());
                            continue 'OUTER;
                        }
                    }
                    Err(WebSocketError::NoDataAvailable) => {
                        break;
                    }
                    // wtf, so much for NoDataAvailable
                    Err(WebSocketError::IoError(ref err))
                        if err.raw_os_error() == Some(libc::EAGAIN) =>
                    {
                        break;
                    }
                    err => {
                        view.global_err(format!("{:?}", err));
                    }
                }
            }
            self.waiter
                .0
                .wait_for(&mut self.waiter.1.lock(), Duration::from_millis(2500));
        }
    }
}
