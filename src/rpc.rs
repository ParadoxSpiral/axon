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

use futures::{Future, Sink, Stream as FutStream};
use futures::future::{self, Either};
use futures::sink::Wait;
use futures::stream::{SplitSink, SplitStream};
use futures::sync::mpsc::{self, Receiver, Sender};
use parking_lot::Mutex;
use serde_json;
use synapse_rpc::message::{CMessage, SMessage};
use tokio::reactor::{Core, Timeout};
use url::Url;
use websocket::ClientBuilder;
use websocket::async::{MessageCodec, Stream};
use websocket::async::client::Framed;
use websocket::message::OwnedMessage;

use std::cell::RefCell;
use std::error::Error;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use view::View;

lazy_static!(
    pub static ref SERVER_VERSION: Mutex<String> = Mutex::new("".to_owned());
);

type InnerStream = Framed<Box<Stream + Send>, MessageCodec<OwnedMessage>>;
type SplitSocket = (
    RefCell<SplitStream<InnerStream>>,
    Mutex<Wait<SplitSink<InnerStream>>>,
);

enum StreamRes {
    Close,
    Msg(OwnedMessage),
}

pub struct RpcContext<'v> {
    socket: RefCell<Option<SplitSocket>>,
    waiter: (RefCell<Sender<()>>, RefCell<Receiver<()>>),
    // FIXME: Once feature `integer atomics` lands, switch to AtomicU64
    serial: AtomicUsize,
    core: RefCell<Core>,
    view: &'v View,
}

unsafe impl<'v> Send for RpcContext<'v> {}
unsafe impl<'v> Sync for RpcContext<'v> {}

impl<'v> RpcContext<'v> {
    pub fn new(view: &'v View) -> RpcContext<'v> {
        RpcContext {
            socket: RefCell::new(None),
            waiter: {
                let (s, r) = mpsc::channel(2);
                (RefCell::new(s), RefCell::new(r))
            },
            serial: AtomicUsize::new(0),
            core: RefCell::new(Core::new().unwrap()),
            view,
        }
    }

    pub fn init(&self, mut srv: Url, pass: &str) -> Result<(), String> {
        #[cfg(feature = "dbg")]
        trace!(*::S_RPC, "Initiating ctx");
        let mut core = self.core.borrow_mut();
        let url = srv.query_pairs_mut().append_pair("password", pass).finish();
        #[allow(unused_mut)]
        let (sink, mut stream) = {
            let timeout = Timeout::new(Duration::from_secs(10), &core.handle()).unwrap();
            let fut = ClientBuilder::new(url.as_str())
                .map_err(|err| format!("{}", err))?
                .async_connect(None, &core.handle())
                .map_err(|err| format!("{:?}", err))
                .select2(timeout.map(|_| "Timeout while connecting to server (10s)".to_owned()));
            match core.run(fut) {
                Ok(Either::A(((client, _), _))) => client.split(),
                Ok(Either::B((err, _))) | Err(Either::A((err, _))) => {
                    return Err(err);
                }
                _ => unreachable!(),
            }
        };

        // synulator doesn't send its RPC version
        #[cfg(not(feature = "synulator"))]
        {
            use synapse_rpc;
            use synapse_rpc::message::Version;

            let timeout = Timeout::new(Duration::from_secs(10), &core.handle()).unwrap();
            let fut = stream
                .by_ref()
                .into_future()
                .map_err(|(err, _)| format!("{:?}", err))
                .select2(
                    timeout.map(|_| "Timeout while waiting for server version (10s)".to_owned()),
                );
            match core.run(fut) {
                Ok(Either::A(((Some(OwnedMessage::Text(msg)), _), _))) => {
                    match serde_json::from_str::<Version>(&msg) {
                        Ok(ver) => {
                            if ver.major != synapse_rpc::MAJOR_VERSION {
                                return Err(format!(
                                    "Server version {:?} incompatible with client {}.{}",
                                    ver,
                                    synapse_rpc::MAJOR_VERSION,
                                    synapse_rpc::MINOR_VERSION
                                ));
                            }
                            (*SERVER_VERSION.lock()) = format!("{}.{}", ver.major, ver.minor);
                        }
                        Err(e) => {
                            return Err(format!("{}", e));
                        }
                    }
                }
                Ok(Either::B((err, _))) | Err(Either::A((err, _))) => {
                    return Err(err);
                }
                _ => unreachable!(),
            }
        }

        *self.socket.borrow_mut() = Some((RefCell::new(stream), Mutex::new(sink.wait())));
        self.wake();
        #[cfg(feature = "dbg")]
        trace!(*::S_RPC, "Initiated ctx");
        Ok(())
    }

    pub fn wake(&self) {
        #[cfg(feature = "dbg")]
        debug!(*::S_RPC, "Should wake");
        self.waiter.0.borrow_mut().try_send(()).unwrap();
    }

    pub fn next_serial(&self) -> u64 {
        #[cfg(feature = "dbg")]
        trace!(*::S_RPC, "Inc serial");
        self.serial.fetch_add(1, Ordering::AcqRel) as _
    }

    pub fn send(&self, msg: CMessage) {
        #[cfg(feature = "dbg")]
        debug!(*::S_RPC, "Sending {:#?}", msg);
        self.send_raw(OwnedMessage::Text(serde_json::to_string(&msg).unwrap()));
    }

    fn send_raw(&self, msg: OwnedMessage) {
        let sink = self.socket.borrow();
        let sink = sink.as_ref();
        let mut sink = sink.unwrap().1.lock();

        match (sink.send(msg), sink.flush()) {
            (Err(e), _) | (_, Err(e)) => self.view.global_err(format!("{:?}", e), Some("RPC")),
            _ => {}
        }
    }

    pub fn recv_until_death(&self) {
        let mut waiter = self.waiter.1.borrow_mut();

        // Each iteration represents the lifetime of a connection to a server
        loop {
            // Wait for initialization
            #[cfg(feature = "dbg")]
            debug!(*::S_RPC, "Waiting for init");
            waiter.by_ref().wait().next().unwrap().unwrap();

            // This scope limits the socket borrow
            {
                // Check if exited before login
                let socket = self.socket.borrow();
                if socket.is_none() {
                    #[cfg(feature = "dbg")]
                    debug!(*::S_RPC, "Quit before login");
                    return;
                }

                let mut core = self.core.borrow_mut();
                let socket = socket.as_ref().unwrap();
                let mut stream = socket.0.borrow_mut();

                let msg_handler = stream
                    .by_ref()
                    .map(|msg| StreamRes::Msg(msg))
                    .map_err(|err| format!("{:?}", err))
                    .select(
                        waiter
                            .by_ref()
                            .map(|_| StreamRes::Close)
                            .map_err(|err| format!("{:?}", err)),
                    )
                    .or_else(|e| future::err(self.view.global_err(e, Some("RPC"))))
                    .and_then(|res| match res {
                        StreamRes::Msg(msg) => match msg {
                            OwnedMessage::Ping(p) => {
                                #[cfg(feature = "dbg")]
                                trace!(*::S_RPC, "Pinged");
                                self.send_raw(OwnedMessage::Pong(p));
                                future::ok(())
                            }
                            OwnedMessage::Close(data) => {
                                #[cfg(feature = "dbg")]
                                debug!(*::S_RPC, "Server closed: {:?}", data);
                                self.view.connection_close(data);
                                future::err(())
                            }
                            OwnedMessage::Text(s) => {
                                let _ = serde_json::from_str::<SMessage>(&s)
                                    .map_err(|e| {
                                        self.view
                                            .global_err(format!("{}", e.description()), Some("RPC"))
                                    })
                                    .map(|msg| match msg {
                                        SMessage::ResourcesExtant { ids, .. } => {
                                            self.send(CMessage::Subscribe {
                                                serial: self.next_serial(),
                                                ids: ids.iter()
                                                    .map(|id| (&**id).to_owned())
                                                    .collect(),
                                            });
                                        }
                                        SMessage::ResourcesRemoved { serial, ids } => {
                                            #[cfg(feature = "dbg")]
                                            debug!(*::S_RPC, "ResourcesRemoved: {:#?}", ids);
                                            self.send(CMessage::Unsubscribe {
                                                serial: self.next_serial(),
                                                ids: ids.clone(),
                                            });

                                            self.view.handle_rpc(
                                                self,
                                                SMessage::ResourcesRemoved { serial, ids },
                                            );
                                        }
                                        _ => {
                                            #[cfg(feature = "dbg")]
                                            debug!(*::S_RPC, "Received: {:#?}", msg);
                                            self.view.handle_rpc(self, msg);
                                        }
                                    });
                                future::ok(())
                            }
                            _ => unreachable!(),
                        },
                        StreamRes::Close => future::err(()),
                    });

                // Wait until the stream is, or should be, terminated
                #[cfg(feature = "dbg")]
                debug!(*::S_RPC, "Running stream handler");
                let _ = core.run(msg_handler.for_each(|_| Ok(())));
            }

            if ::RUNNING.load(Ordering::Acquire) {
                #[cfg(feature = "dbg")]
                info!(*::S_RPC, "Rejiggering for new server");
                *self.socket.borrow_mut() = None;
                continue;
            } else {
                #[cfg(feature = "dbg")]
                info!(*::S_RPC, "Terminating RPC");
                break;
            }
        }
    }
}
