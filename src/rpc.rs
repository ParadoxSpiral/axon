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
use log::{debug, trace, warn};
use parking_lot::Mutex;
use serde_json;
use synapse_rpc::{
    self,
    message::{CMessage, SMessage},
};
use tokio::{net::TcpStream, prelude::*};
use tokio_tungstenite::{self, tungstenite::Message as WsMessage, MaybeTlsStream, WebSocketStream};
use url::Url;

use std::{
    error::Error,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

static SERIAL: AtomicUsize = AtomicUsize::new(0);

pub type WsSink = Arc<Mutex<stream::SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>>>>;

pub enum Item {
    Idle,
    Msg(SMessage<'static>),
}

pub fn next_serial() -> u64 {
    SERIAL.fetch_add(1, Ordering::Relaxed) as _
}

pub fn send(sink: &WsSink, msg: CMessage) {
    debug!("Sending {:#?}", msg);
    send_raw(
        Arc::clone(sink),
        WsMessage::Text(serde_json::to_string(&msg).unwrap()),
    );
}

fn send_raw(sink: WsSink, msg: WsMessage) {
    let mut msg = Some(msg);
    tokio::spawn(future::poll_fn(move || {
        if SERIAL.load(Ordering::Acquire) == 0 {
            Ok(Async::Ready(()))
        } else if let Some(msg2) = msg.take() {
            let mut sink = sink.lock();
            if let AsyncSink::NotReady(msg2) = sink.start_send(msg2).unwrap() {
                msg = Some(msg2);
                Ok(Async::NotReady)
            } else {
                Ok(sink.poll_complete().unwrap())
            }
        } else {
            Ok(sink.lock().poll_complete().unwrap())
        }
    }));
}

pub fn connections(
    urls: mpsc::Receiver<(String, String)>,
) -> impl Stream<
    Item = impl Future<
        Item = (WsSink, impl Stream<Item = Item, Error = (String, String)>),
        Error = (String, String),
    >,
    Error = (String, String),
> {
    urls.map_err(|_| unreachable!())
        .and_then(move |(server, pass)| {
            let mut url = Url::parse(&server).map_err(|e| ("Url".into(), e.to_string()))?;
            url.query_pairs_mut()
                .append_pair("password", &pass)
                .finish();
            trace!("Should connect to {:?}", url.origin());

            Ok(tokio_tungstenite::connect_async(url)
                .timeout(Duration::from_secs(10))
                .map_err(|e| {
                    (
                        "RPC".to_string(),
                        if e.is_timer() {
                            "Timeout connecting to server (10s)".to_owned()
                        } else {
                            format!("{:?}", e.into_inner().unwrap())
                        },
                    )
                })
                .map(move |(stream, _)| {
                    trace!("Connected");

                    let (sink, stream) = stream.split();
                    let sink = Arc::new(Mutex::new(sink));
                    let stream = handle_connection(
                        Arc::clone(&sink),
                        stream.map_err(|e| ("RPC".into(), e.to_string())),
                    );
                    (sink, stream)
                }))
        })
}

fn handle_connection(
    sink: WsSink,
    stream: impl Stream<Item = WsMessage, Error = (String, String)>,
) -> impl Stream<Item = Item, Error = (String, String)> {
    stream
        .and_then(move |msg| match msg {
            WsMessage::Ping(p) => {
                send_raw(Arc::clone(&sink), WsMessage::Pong(p));
                Ok(Item::Idle)
            }
            WsMessage::Text(s) => match serde_json::from_str::<SMessage>(&s) {
                Err(e) => Err(("RPC".to_owned(), e.description().to_string())),
                Ok(SMessage::ResourcesExtant { ids, .. }) => {
                    trace!("ResourcesExtant: {:#?}", ids);
                    send(
                        &sink,
                        CMessage::Subscribe {
                            serial: next_serial(),
                            ids: ids.iter().map(|id| (&**id).to_string()).collect(),
                        },
                    );
                    // FIXME: This shouldn't be necessary, but without it we miss the reply - why?
                    task::current().notify();
                    Ok(Item::Idle)
                }
                Ok(SMessage::ResourcesRemoved { serial, ids }) => {
                    trace!("ResourcesRemoved: {:#?}", ids);
                    send(
                        &sink,
                        CMessage::Unsubscribe {
                            serial: next_serial(),
                            ids: ids.clone(),
                        },
                    );
                    Ok(Item::Msg(SMessage::ResourcesRemoved { serial, ids }))
                }
                Ok(SMessage::RpcVersion(ver)) => {
                    if ver.major != synapse_rpc::MAJOR_VERSION
                        || (ver.minor != synapse_rpc::MINOR_VERSION
                            && synapse_rpc::MAJOR_VERSION == 0)
                    {
                        warn!("RPC version mismatch");
                        Err((
                            "RPC".to_string(),
                            format!(
                                "Server version {:?} \
                                 incompatible with client {}.{}",
                                ver,
                                synapse_rpc::MAJOR_VERSION,
                                synapse_rpc::MINOR_VERSION
                            ),
                        ))
                    } else {
                        Ok(Item::Msg(SMessage::RpcVersion(ver)))
                    }
                }
                Ok(msg) => {
                    trace!("Received: {:#?}", msg);
                    Ok(Item::Msg(msg))
                }
            },
            _ => unreachable!(),
        })
        .or_else(move |v| {
            SERIAL.store(0, Ordering::Release);
            Err(v)
        })
}
