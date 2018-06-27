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

use futures::future;
use futures::sink::Wait;
use futures::stream::{SplitSink, SplitStream};
use futures::sync::mpsc::{self, Receiver, Sender};
use futures::{Future, Sink, Stream as FutStream};
use parking_lot::Mutex;
use serde_json;
use synapse_rpc;
use synapse_rpc::message::{CMessage, SMessage};
use tokio::util::FutureExt;
use tokio_core::reactor::Core;
use url::Url;
use websocket::async::client::Framed;
use websocket::async::{MessageCodec, Stream};
use websocket::message::OwnedMessage;
use websocket::{ClientBuilder, CloseData};

use std::cell::RefCell;
use std::error::Error;
use std::mem::ManuallyDrop;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tui::View;

type InnerStream = Framed<Box<Stream + Send>, MessageCodec<OwnedMessage>>;
type SplitSocket = (SplitStream<InnerStream>, Wait<SplitSink<InnerStream>>);

enum StreamRes {
    Msg(OwnedMessage),
    Idle,
    Close,
}

enum WaiterMsg {
    // Server, pass
    Init(String, String),
    Send(OwnedMessage),
    Close,
}

thread_local!(
    static SOCKET: RefCell<Option<SplitSocket>> = RefCell::new(None);
    // For some reason the dtors fail to function and panic, so we let the OS do the cleanup
    static CORE: ManuallyDrop<RefCell<Core>>
                = ManuallyDrop::new(RefCell::new(Core::new().unwrap()));
);

pub struct RpcContext {
    waiter: (Mutex<Sender<WaiterMsg>>, Mutex<Receiver<WaiterMsg>>),
    // FIXME: Once feature `integer atomics` lands, switch to AtomicU64
    serial: AtomicUsize,
}

impl RpcContext {
    pub fn new() -> RpcContext {
        RpcContext {
            waiter: {
                let (s, r) = mpsc::channel(10);
                (Mutex::new(s), Mutex::new(r))
            },
            serial: AtomicUsize::new(0),
        }
    }

    pub fn start_init(&self, srv: String, pass: String) {
        #[cfg(feature = "dbg")]
        trace!(*::S_RPC, "RPC should init");

        self.waiter
            .0
            .lock()
            .try_send(WaiterMsg::Init(srv, pass))
            .unwrap();
    }

    fn init(&self, mut srv: Url, pass: &str) -> Result<(), String> {
        #[cfg(feature = "dbg")]
        trace!(*::S_RPC, "Initiating ctx");
        let url = srv.query_pairs_mut().append_pair("password", pass).finish();
        CORE.with(|core| {
            let mut core = core.borrow_mut();
            let (sink, stream) = {
                let fut = ClientBuilder::from_url(url)
                    .async_connect(None, &core.handle())
                    .map_err(|err| format!("{:?}", err))
                    .deadline(Instant::now() + Duration::from_secs(10))
                    .map_err(|e| {
                        if e.is_timer() {
                            "Timeout while connecting to server (10s)".to_owned()
                        } else {
                            e.into_inner().unwrap()
                        }
                    });

                match core.run(fut) {
                    Ok(client) => client.0.split(),
                    // Need to convert error types here
                    Err(err) => {
                        return Err(err);
                    }
                }
            };
            SOCKET.with(|s| {
                *s.borrow_mut() = Some((stream, sink.wait()));
                #[cfg(feature = "dbg")]
                trace!(*::S_RPC, "Initiated ctx");
            });
            Ok(())
        })
    }

    pub fn disconnect(&self) {
        #[cfg(feature = "dbg")]
        debug!(*::S_RPC, "RPC should disconnect");
        self.waiter.0.lock().try_send(WaiterMsg::Close).unwrap();
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
        self.waiter.0.lock().try_send(WaiterMsg::Send(msg)).unwrap();
    }

    pub fn recv_until_death(&self, view: &View) {
        // Each iteration represents the lifetime of a connection to a server
        loop {
            // Wait for initialization
            #[cfg(feature = "dbg")]
            debug!(*::S_RPC, "Waiting for init");
            match (*self.waiter.1.lock())
                .by_ref()
                .wait()
                .next()
                .unwrap()
                .unwrap()
            {
                WaiterMsg::Init(srv, pass) => {
                    match Url::parse(&srv) {
                        Err(e) => {
                            view.global_err(e, Some("Url".to_owned()));
                            continue;
                        }
                        Ok(srv) => {
                            if let Err(e) = self.init(srv, &pass) {
                                view.global_err(e, Some("RPC".to_owned()));
                                continue;
                            }
                        }
                    }
                    view.login(&self);
                }
                WaiterMsg::Close => {
                    #[cfg(feature = "dbg")]
                    info!(*::S_RPC, "Terminating RPC");
                    break;
                }
                _ => unreachable!(),
            }

            SOCKET.with(|socket| {
                CORE.with(|core| {
                    let mut core = core.borrow_mut();
                    let mut socket = socket.borrow_mut();
                    let socket = socket.as_mut().unwrap();
                    let stream = &mut socket.0;
                    let sink = &mut socket.1;

                    let mut waiter = self.waiter.1.lock();

                    let msg_handler = stream
                        .by_ref()
                        .map(|msg| StreamRes::Msg(msg))
                        .map_err(|err| format!("{:?}", err))
                        .select(
                            waiter
                                .by_ref()
                                .map(|msg| match msg {
                                    WaiterMsg::Init(_, _) => unreachable!(),
                                    WaiterMsg::Close => StreamRes::Close,
                                    WaiterMsg::Send(msg) => {
                                        // TODO: Make async
                                        match (sink.send(msg), sink.flush()) {
                                            (Err(e), _) | (_, Err(e)) => {
                                                view.global_err(format!("{:?}", e), Some("RPC"))
                                            }
                                            _ => {}
                                        }
                                        StreamRes::Idle
                                    }
                                })
                                .map_err(|err| format!("{:?}", err)),
                        )
                        .or_else(|e| future::err(view.global_err(e, Some("RPC"))))
                        .and_then(|res| match res {
                            StreamRes::Idle => future::ok(()),
                            StreamRes::Close => future::err(()),
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
                                    view.connection_close(data);
                                    future::err(())
                                }
                                OwnedMessage::Text(s) => match serde_json::from_str::<SMessage>(&s)
                                {
                                    Err(e) => {
                                        view.global_err(
                                            format!("{}", e.description()),
                                            Some("RPC"),
                                        );
                                        future::ok(())
                                    }
                                    Ok(msg) => match msg {
                                        SMessage::ResourcesExtant { ids, .. } => {
                                            self.send(CMessage::Subscribe {
                                                serial: self.next_serial(),
                                                ids: ids
                                                    .iter()
                                                    .map(|id| (&**id).to_owned())
                                                    .collect(),
                                            });
                                            future::ok(())
                                        }
                                        SMessage::ResourcesRemoved { serial, ids } => {
                                            #[cfg(feature = "dbg")]
                                            debug!(*::S_RPC, "ResourcesRemoved: {:#?}", ids);
                                            self.send(CMessage::Unsubscribe {
                                                serial: self.next_serial(),
                                                ids: ids.clone(),
                                            });

                                            view.handle_rpc(
                                                self,
                                                SMessage::ResourcesRemoved { serial, ids },
                                            );
                                            future::ok(())
                                        }
                                        SMessage::RpcVersion(ver) => {
                                            if ver.major != synapse_rpc::MAJOR_VERSION
                                                || (ver.minor != synapse_rpc::MINOR_VERSION
                                                    && synapse_rpc::MAJOR_VERSION == 0)
                                            {
                                                view.connection_close(Some(CloseData::new(
                                                    1,
                                                    format!(
                                                        "Server version {:?} \
                                                         incompatible with client {}.{}",
                                                        ver,
                                                        synapse_rpc::MAJOR_VERSION,
                                                        synapse_rpc::MINOR_VERSION
                                                    ),
                                                )));
                                                #[cfg(feature = "dbg")]
                                                warn!(*::S_RPC, "RPC version mismatch");
                                                future::err(())
                                            } else {
                                                #[cfg(feature = "dbg")]
                                                debug!(*::S_RPC, "RPC version match");
                                                view.handle_rpc(self, SMessage::RpcVersion(ver));
                                                future::ok(())
                                            }
                                        }
                                        _ => {
                                            #[cfg(feature = "dbg")]
                                            debug!(*::S_RPC, "Received: {:#?}", msg);
                                            view.handle_rpc(self, msg);
                                            future::ok(())
                                        }
                                    },
                                },
                                _ => unreachable!(),
                            },
                        });

                    // Wait until the stream is, or should be, terminated
                    #[cfg(feature = "dbg")]
                    debug!(*::S_RPC, "Running stream handler");
                    let _ = core.run(msg_handler.for_each(|_| Ok(())));
                });

                #[cfg(feature = "dbg")]
                info!(*::S_RPC, "Rejiggering for new server");
                *socket.borrow_mut() = None;
            });
        }
    }
}
