// This file is part of Axon.
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
use termion::{self, clear, cursor, event::Key, raw::IntoRawMode, screen::AlternateScreen};
use tokio::{prelude::*, timer};
use tokio_signal::unix::Signal;

use std::{
    cmp,
    io::{self, Write},
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{
    input,
    rpc::{Item as RpcItem, WsSink},
    tui::{panels, widgets, Component, InputResult},
    utils::{align, color::ColorEscape},
};

enum Err {
    Shutdown,
    Recoverable((String, String)),
    Unrecoverable((String, String)),
}

enum Connection<E, P>
where
    E: Stream + Sized,
    P: Future + Sized,
{
    Idle,
    Established(E),
    Pending(P),
}

pub fn run(
    mut urls: mpsc::Sender<(String, String)>,
    mut conns: impl Stream<
        Item = impl Future<
            Item = (
                WsSink,
                impl Stream<Item = RpcItem, Error = (String, String)>,
            ),
            Error = (String, String),
        >,
        Error = (String, String),
    >,
) -> impl Future<Item = (), Error = ()> {
    let size = termion::terminal_size().unwrap_or((0, 0));
    let mut render_buffer = Vec::with_capacity(size.0 as usize * size.1 as usize + 1);
    // FIXME: Use an unbuffered stdout: `https://github.com/rust-lang/rust/issues/58326`,
    // to avoid the LineWriter
    let mut out = AlternateScreen::from(io::stdout()).into_raw_mode().unwrap();
    write!(out, "{}", cursor::Hide).unwrap();

    // Wrap the things that are sent into Futures and shared in Arcs/Mutexes
    let conn1 = Arc::new(Mutex::new(Connection::Idle));
    let conn2 = Arc::clone(&conn1);
    let logged_in1 = Arc::new(Mutex::new(false));
    let logged_in2 = Arc::clone(&logged_in1);
    let content1 = Arc::new(Mutex::new(Some(
        Box::new(panels::Login::new()) as Box<Component>
    )));
    let content2 = Arc::clone(&content1);
    let content3 = Arc::clone(&content1);
    let content4 = Arc::clone(&content1);

    let interval = timer::Interval::new(Instant::now(), Duration::from_secs(10))
        .map_err(|e| Err::Unrecoverable(("Timer".to_string(), e.to_string())))
        .map(|_| true);

    // SIGWINCH is signalled if the terminal got resized
    // TODO: Do layouting here
    let resize = Signal::new(libc::SIGWINCH)
        .flatten_stream()
        .map_err(|e| Err::Unrecoverable(("Signal".to_string(), e.to_string())))
        .map(|_| true);

    let input = input::stream()
        .map_err(Err::Unrecoverable)
        .and_then(move |key| match key {
            Key::Ctrl('q') => {
                let mut logged_in = logged_in1.lock();
                if *logged_in {
                    debug!("Disconnecting");

                    let mut conn = conn1.lock();
                    let mut content = content1.lock();
                    *conn = Connection::Idle;
                    *content = Some(Box::new(panels::Login::new()));
                    *logged_in = false;

                    Ok(true)
                } else {
                    debug!("Quitting");
                    Err(Err::Shutdown)
                }
            }
            key => {
                let (w, h) = termion::terminal_size().unwrap_or((0, 0));
                let mut content = content1.lock();
                match content
                    .as_mut()
                    .unwrap_or_else(|| unreachable!())
                    .input(key, w, h)
                {
                    InputResult::ReplaceWith(other) => {
                        *content = Some(other);
                        Ok(true)
                    }
                    InputResult::ConnectWith(svr, pass) => {
                        urls.try_send((svr, pass)).unwrap();
                        Ok(false)
                    }
                    InputResult::Rerender => Ok(true),
                    _ => Ok(false),
                }
            }
        });

    let rpc = stream::poll_fn(move || {
        let mut conn = conn2.lock();
        match *conn {
            // Here we assume that a new connection will only be made while idle,
            // i.e. on the login screen
            Connection::Idle => match conns.poll() {
                Err(e) => std::result::Result::Err(Err::Recoverable(e)),
                Ok(Async::Ready(Some(fut))) => {
                    *conn = Connection::Pending(fut);
                    Ok(Async::NotReady)
                }
                _ => Ok(Async::NotReady),
            },
            Connection::Pending(ref mut c) => match c.poll() {
                Err(e) => {
                    *conn = Connection::Idle;
                    std::result::Result::Err(Err::Recoverable(e))
                }
                Ok(Async::Ready((sink, stream))) => {
                    *conn = Connection::Established(stream);

                    let mut content = content2.lock();
                    let height = termion::terminal_size().unwrap_or((0, 0)).1;
                    *content = Some(Box::new(panels::Main::new(&sink, height)));

                    let mut logged_in = logged_in2.lock();
                    *logged_in = true;

                    Ok(Async::Ready(Some(true)))
                }
                _ => Ok(Async::NotReady),
            },
            Connection::Established(ref mut c) => match c.poll() {
                Err(e) => {
                    let mut content = content2.lock();
                    let mut logged_in = logged_in2.lock();
                    *content = Some(Box::new(panels::Login::new()));
                    *logged_in = false;
                    *conn = Connection::Idle;

                    std::result::Result::Err(Err::Recoverable(e))
                }
                Ok(Async::Ready(Some(RpcItem::Msg(msg)))) => {
                    let mut content = content2.lock();
                    let content = content.as_mut().unwrap_or_else(|| unreachable!());
                    Ok(Async::Ready(Some(content.rpc(msg))))
                }
                _ => Ok(Async::NotReady),
            },
        }
    });

    // This futurefied stream first selects on:
    // 1) stdin input
    // 2) rpc activity
    // 3) SIGWINCH, to handle resizing
    // 4) a 10s interval, to regularly update the server uptime
    // If no error occured, the selected value is a bool that if true causes a rendering pass
    // handled via a for_each.
    // In case of an error it is checked what kind of error: Shutdown. Recoverable, or Unrecoverable.
    // In case of a recoverable error, we construct an overlay box with the error location and text
    // on the topmost component, otherwise the error is handed to the rendering for_each which
    // stops it and thus stops the whole Future/application.
    // Before the application stops, all internally spawned tasks are waited upon, so any remaining
    // rpc msg send operations are completed.
    input
        .select(rpc.select(resize.select(interval)))
        .or_else(move |e| match e {
            Err::Recoverable((name, text)) => {
                warn!("Recoverable err in {}: {}", name, text);

                let mut content = content3.lock();
                let len = cmp::max(text.len(), name.len()) + 2;
                let overlay = Box::new(widgets::OwnedOverlay::new(
                    widgets::CloseOnInput::new(
                        widgets::IgnoreRpc::new(
                            widgets::Text::<_, align::x::Center, align::y::Top>::new(true, text),
                        ),
                        &[
                            Key::Esc,
                            Key::Backspace,
                            Key::Delete,
                            Key::Char('q'),
                            Key::Char('\n'),
                        ],
                    ),
                    content.take().unwrap_or_else(|| unreachable!()),
                    (len as _, 1),
                    Some(ColorEscape::red()),
                    Some(name),
                ));
                *content = Some(overlay);

                Ok(true)
            }
            e => Err(e),
        })
        .for_each(move |render| {
            let err = |t: io::Error| Err::Unrecoverable(("Render".to_string(), t.to_string()));
            if render {
                trace!("Rendering");
                if let Ok((width, height)) = termion::terminal_size() {
                    let mut content = content4.lock();
                    let content = content.as_mut().unwrap_or_else(|| unreachable!());
                    write!(render_buffer, "{}", clear::All).map_err(err)?;
                    content.render(&mut render_buffer, width, height, 1, 1);

                    out.write_all(&*render_buffer).map_err(err)?;
                    out.flush().map_err(err)?;
                    render_buffer.clear();
                } else {
                    write!(out, "smol").map_err(err)?;
                    out.flush().map_err(err)?;
                }
            }
            Ok(())
        })
        .map_err(|e| match e {
            Err::Shutdown => (),
            Err::Recoverable(_) => unreachable!(),
            Err::Unrecoverable((name, text)) => {
                warn!("Unrecoverable error in {}: {}", name, text);
                println!("Unrecoverable error in {}: {}", name, text);
            }
        })
        .then(|_| {
            debug!("View finishing");
            print!("{}", cursor::Show);
            Ok(())
        })
}
