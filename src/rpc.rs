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

use futures::sync::mpsc;
use parking_lot::Mutex;
use serde_json;
use synapse_rpc;
use synapse_rpc::message::{CMessage, SMessage};
use tokio::prelude::*;
use url::Url;
use ws;
use ws::tungstenite::Message;

use std::error::Error;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tui::view::Notify;
use utils::color::ColorEscape;

enum WaiterMsg {
    Send(Message),
    Close,
}

lazy_static! {
    static ref WAKER: (Mutex<mpsc::Sender<WaiterMsg>>, Mutex<mpsc::Receiver<WaiterMsg>>) = {
        let (s, r) = mpsc::channel(10);
        (Mutex::new(s), Mutex::new(r))
    };
    // TODO; Swap to AtomicU64 once stable
    static ref SERIAL: AtomicUsize = AtomicUsize::new(0);
}

pub fn next_serial() -> u64 {
    SERIAL.fetch_add(1, Ordering::AcqRel) as _
}

pub fn send(msg: CMessage) {
    #[cfg(feature = "dbg")]
    debug!(*::S_RPC, "Sending {:#?}", msg);
    send_raw(Message::Text(serde_json::to_string(&msg).unwrap()));
}

fn send_raw(msg: Message) {
    WAKER.0.lock().try_send(WaiterMsg::Send(msg)).unwrap();
}

pub fn disconnect() {
    #[cfg(feature = "dbg")]
    debug!(*::S_RPC, "RPC should disconnect");
    WAKER.0.lock().try_send(WaiterMsg::Close).unwrap();
}

pub fn start_connect(srv: &str, pass: &str) -> Option<impl Future<Item = (), Error = ()>> {
    #[cfg(feature = "dbg")]
    trace!(*::S_RPC, "RPC should connect");

    let mut url = match Url::parse(srv) {
        Ok(u) => u,
        Err(e) => {
            Notify::overlay("Url".to_owned(), e.to_string(), Some(ColorEscape::red()));
            return None;
        }
    };
    url.query_pairs_mut().append_pair("password", pass).finish();

    Some(
        ws::connect_async(url)
            .map_err(|err| format!("{:?}", err))
            .timeout(Duration::from_secs(10))
            .map_err(|e| {
                Notify::overlay(
                    "RPC".into(),
                    if e.is_timer() {
                        "Timeout while connecting to server (10s)".to_owned()
                    } else {
                        e.into_inner().unwrap()
                    },
                    Some(ColorEscape::red()),
                );
            })
            .map(move |(stream, _)| {
                #[cfg(feature = "dbg")]
                trace!(*::S_RPC, "RPC connected");

                Notify::login();

                let (sink, stream) = stream.split();
                let sink1 = Arc::new(Mutex::new(sink));
                let sink2 = Arc::clone(&sink1);
                let mut pending_flush = false;
                tokio::spawn(
                    stream
                        .map_err(|e| {
                            Notify::overlay(
                                "RPC".to_owned(),
                                e.to_string(),
                                Some(ColorEscape::red()),
                            );
                        })
                        .select(
                            future::poll_fn(move || {
                                let mut waker = WAKER.1.lock();
                                loop {
                                    match waker.poll() {
                                        Ok(Async::Ready(Some(WaiterMsg::Close))) => {
                                            return Err(());
                                        }
                                        Ok(Async::Ready(Some(WaiterMsg::Send(msg)))) => {
                                            if let AsyncSink::NotReady(msg) =
                                                sink1.lock().start_send(msg).unwrap()
                                            {
                                                send_raw(msg);
                                            } else {
                                                pending_flush = true;
                                            }
                                        }
                                        Ok(Async::NotReady) if pending_flush => {
                                            let sink = Arc::clone(&sink1);
                                            tokio::spawn(future::poll_fn(move || {
                                                if SERIAL.load(Ordering::Acquire) == 0 {
                                                    Ok(Async::Ready(()))
                                                } else {
                                                    sink.lock().poll_complete().map_err(|e| {
                                                        Notify::overlay(
                                                            "RPC".to_owned(),
                                                            e.to_string(),
                                                            Some(ColorEscape::red()),
                                                        );
                                                    })
                                                }
                                            }));
                                            pending_flush = false;
                                            return Ok(Async::NotReady);
                                        }
                                        Ok(Async::NotReady) => {
                                            return Ok(Async::NotReady);
                                        }
                                        _ => {
                                            return Err(());
                                        }
                                    }
                                }
                            })
                            .into_stream(),
                        )
                        .for_each(|msg| match msg {
                            Message::Ping(p) => Ok(send_raw(Message::Pong(p))),
                            Message::Text(s) => match serde_json::from_str::<SMessage>(&s) {
                                Err(e) => {
                                    Notify::overlay(
                                        "RPC".to_owned(),
                                        e.description().to_string(),
                                        Some(ColorEscape::red()),
                                    );
                                    Err(())
                                }
                                Ok(msg) => match msg {
                                    SMessage::ResourcesExtant { ids, .. } => {
                                        Ok(send(CMessage::Subscribe {
                                            serial: next_serial(),
                                            ids: ids.iter().map(|id| (&**id).to_owned()).collect(),
                                        }))
                                    }
                                    SMessage::ResourcesRemoved { serial, ids } => {
                                        #[cfg(feature = "dbg")]
                                        debug!(*::S_RPC, "ResourcesRemoved: {:#?}", ids);

                                        send(CMessage::Unsubscribe {
                                            serial: next_serial(),
                                            ids: ids.clone(),
                                        });
                                        Notify::rpc(SMessage::ResourcesRemoved { serial, ids });
                                        Ok(())
                                    }
                                    SMessage::RpcVersion(ver) => {
                                        if ver.major != synapse_rpc::MAJOR_VERSION
                                            || (ver.minor != synapse_rpc::MINOR_VERSION
                                                && synapse_rpc::MAJOR_VERSION == 0)
                                        {
                                            #[cfg(feature = "dbg")]
                                            warn!(*::S_RPC, "RPC version mismatch");

                                            Notify::overlay(
                                                "RPC".to_string(),
                                                format!(
                                                    "Server version {:?} \
                                                     incompatible with client {}.{}",
                                                    ver,
                                                    synapse_rpc::MAJOR_VERSION,
                                                    synapse_rpc::MINOR_VERSION
                                                ),
                                                Some(ColorEscape::red()),
                                            );
                                            Err(())
                                        } else {
                                            Notify::rpc(SMessage::RpcVersion(ver));
                                            Ok(())
                                        }
                                    }
                                    _ => {
                                        #[cfg(feature = "dbg")]
                                        debug!(*::S_RPC, "Received: {:#?}", msg);

                                        Notify::rpc(msg);
                                        Ok(())
                                    }
                                },
                            },
                            _ => unreachable!(),
                        })
                        .map_err(move |_| {
                            Notify::close();

                            SERIAL.store(0, Ordering::Release);
                            sink2.lock().close().unwrap();

                            // Drain remaining sends
                            let mut waker = WAKER.1.lock();
                            while let Ok(Async::Ready(_)) = waker.poll() {}

                            #[cfg(feature = "dbg")]
                            debug!(*::S_RPC, "RPC disconnected");
                        }),
                );
            }),
    )
}
