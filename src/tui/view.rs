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
use parking_lot::Mutex;
use synapse_rpc::message::SMessage;
use termion::event::Key;
use termion::raw::{IntoRawMode, RawTerminal};
use termion::{self, clear, color, cursor};
use tokio::prelude::*;
use tokio::timer;

use std::cmp;
use std::io::{self, Stdout, Write};
use std::mem;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::{panels, widgets, Component, InputResult};
use rpc;
use utils::align;

lazy_static! {
    static ref RPC: (
        Mutex<mpsc::UnboundedSender<SMessage<'static>>>,
        Mutex<mpsc::UnboundedReceiver<SMessage<'static>>>
    ) = {
        let (s, r) = mpsc::unbounded();
        (Mutex::new(s), Mutex::new(r))
    };
    static ref INPUT: (
        Mutex<mpsc::UnboundedSender<Key>>,
        Mutex<mpsc::UnboundedReceiver<Key>>
    ) = {
        let (s, r) = mpsc::unbounded();
        (Mutex::new(s), Mutex::new(r))
    };
}

pub fn notify_rpc(msg: SMessage<'static>) {
    // We don't care about the error, because it only signifies closal, which implies shutdown
    let _ = RPC.0.lock().unbounded_send(msg);
}

pub fn notify_input(k: Key) {
    // We don't care about the error, because it only signifies closal, which implies shutdown
    let _ = INPUT.0.lock().unbounded_send(k);
}

pub struct View {
    content: Mutex<Box<Component>>,
    logged_in: AtomicBool,
    render_buf: Mutex<Vec<u8>>,
    stdout: Mutex<RawTerminal<Stdout>>,
}

impl View {
    pub fn new() -> View {
        let mut interv = timer::Interval::new(Instant::now(), Duration::from_secs(1));
        ::EXECUTOR.spawn(
            future::poll_fn(move || {
                let mut ct = ::VIEW.content.lock();
                let mut inp = INPUT.1.lock();
                let mut rpc = RPC.1.lock();

                let mut render = false;
                loop {
                    let (k, msg) = match (interv.poll(), inp.poll(), rpc.poll()) {
                        (Err(_), _, _) | (_, Err(_), _) | (_, _, Err(_)) => {
                            rpc::disconnect();
                            rpc.close();
                            inp.close();
                            return Err(());
                        }
                        (_, Ok(Async::Ready(k)), Ok(Async::Ready(msg))) => (k, msg),
                        (_, Ok(Async::Ready(k)), _) => (k, None),
                        (_, _, Ok(Async::Ready(msg))) => (None, msg),
                        (Ok(Async::Ready(_)), _, _) => {
                            render = true;
                            (None, None)
                        }
                        _ => {
                            break;
                        }
                    };

                    if let Some(k) = k {
                        match ::VIEW.handle_input(&mut *ct, k) {
                            InputResult::Rerender => {
                                render = true;
                            }
                            InputResult::Close => {
                                rpc.close();
                                inp.close();
                                return Ok(Async::Ready(()));
                            }
                            _ => (),
                        }
                    }
                    if let Some(msg) = msg {
                        if ct.rpc(msg) {
                            render = true;
                        }
                    }
                }

                if render {
                    ::VIEW.render(&mut **ct);
                }

                Ok(Async::NotReady)
            }).then(|v| {
                #[cfg(feature = "dbg")]
                debug!(*::S_VIEW, "View finishing");
                v
            }),
        );

        let size = termion::terminal_size().unwrap_or((0, 0));
        let mut rb = Vec::with_capacity(size.0 as usize * size.1 as usize + 1);
        write!(rb, "{}", cursor::Hide).unwrap();

        View {
            content: Mutex::new(Box::new(panels::LoginPanel::new())),
            logged_in: AtomicBool::new(false),
            render_buf: Mutex::new(rb),
            stdout: Mutex::new(io::stdout().into_raw_mode().unwrap()),
        }
    }

    // Called by RPC to signify successful login
    pub fn login(&self) {
        let mut ct = self.content.lock();
        *ct = Box::new(panels::MainPanel::new());
        self.logged_in.store(true, Ordering::Release);
    }

    fn render(&self, ct: &mut Component) {
        let mut out = self.stdout.lock();

        if let Ok((width, height)) = termion::terminal_size() {
            let mut buf = self.render_buf.lock();

            write!(buf, "{}", clear::All).unwrap();
            ct.render(&mut buf, width, height, 1, 1);

            out.write_all(&*buf).unwrap();
            out.flush().unwrap();
            buf.clear();
        } else {
            write!(out, "smol").unwrap();
            out.flush().unwrap();
        }
    }

    fn handle_input(&self, ct: &mut Box<Component>, k: Key) -> InputResult {
        match k {
            Key::Ctrl('q') => if self.logged_in.load(Ordering::Acquire) {
                #[cfg(feature = "dbg")]
                debug!(*::S_VIEW, "Disconnecting");

                rpc::disconnect();
                self.internal_connection_close(ct);

                InputResult::Rerender
            } else {
                #[cfg(feature = "dbg")]
                debug!(*::S_VIEW, "Closing");

                InputResult::Close
            },
            _ => {
                let s = termion::terminal_size().unwrap_or((0, 0));
                match ct.input(k, s.0, s.1) {
                    InputResult::ReplaceWith(other) => {
                        mem::replace(&mut *ct, other);
                        InputResult::Rerender
                    }
                    res => res,
                }
            }
        }
    }

    pub fn overlay<C: color::Color + Send + Sync + 'static>(
        &self,
        name: String,
        text: String,
        color: Option<C>,
    ) {
        self.internal_overlay(&mut *self.content.lock(), name, text, color)
    }

    fn internal_overlay<C: color::Color + Send + Sync + 'static>(
        &self,
        ct: &mut Box<Component>,
        name: String,
        text: String,
        color: Option<C>,
    ) {
        // Because we can't move the component out, we need to use this hack
        // This is only safe because we leak ct below, so that it won't get freed
        let alias_cur = unsafe { Box::from_raw((&mut **ct) as *mut Component) };

        let len = cmp::max(text.len(), name.len()) + 2;
        let overlay = Box::new(widgets::OwnedOverlay::<_, C>::new(
            widgets::CloseOnInput::new(
                widgets::IgnoreRpc::new(widgets::Text::<_, align::x::Center, align::y::Top>::new(
                    true, text,
                )),
                &[
                    Key::Esc,
                    Key::Backspace,
                    Key::Delete,
                    Key::Char('q'),
                    Key::Char('\n'),
                ],
            ),
            alias_cur,
            (len as _, 1),
            color,
            Some(name),
        ));
        mem::forget(mem::replace(&mut *ct, overlay));

        self.render(&mut **ct);
    }

    pub fn connection_close(&self) {
        let mut ct = self.content.lock();
        self.internal_connection_close(&mut *ct);
    }

    fn internal_connection_close(&self, ct: &mut Box<Component>) {
        self.logged_in.store(false, Ordering::Release);
        *ct = Box::new(panels::LoginPanel::new());
    }
}
