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
use termion::raw::IntoRawMode;
use termion::{self, clear, cursor};
use tokio::prelude::*;
use tokio::timer;

use std::cmp;
use std::io::{self, Write};
use std::mem;
use std::process;
use std::time::{Duration, Instant};

use super::{panels, widgets, Component, InputResult};
use rpc;
use utils::{align, color::ColorEscape};

lazy_static! {
    static ref NOTIFICATIONS: (
        Mutex<mpsc::UnboundedSender<Notify>>,
        Mutex<Option<mpsc::UnboundedReceiver<Notify>>>
    ) = {
        let (s, r) = mpsc::unbounded();
        (Mutex::new(s), Mutex::new(Some(r)))
    };
}

pub enum Notify {
    Login,
    Close,
    Input(Key),
    Rpc(SMessage<'static>),
    // name, text, color
    Overlay(String, String, Option<ColorEscape>),
}

// We don't care about the errors, because it only signifies closal, which implies shutdown
#[allow(unused_must_use)]
impl Notify {
    pub fn login() {
        NOTIFICATIONS.0.lock().unbounded_send(Notify::Login);
    }
    pub fn close() {
        NOTIFICATIONS.0.lock().unbounded_send(Notify::Close);
    }
    pub fn input(key: Key) {
        NOTIFICATIONS.0.lock().unbounded_send(Notify::Input(key));
    }
    pub fn rpc(msg: SMessage<'static>) {
        NOTIFICATIONS.0.lock().unbounded_send(Notify::Rpc(msg));
    }
    pub fn overlay(name: String, text: String, color: Option<ColorEscape>) {
        NOTIFICATIONS
            .0
            .lock()
            .unbounded_send(Notify::Overlay(name, text, color));
    }
}

pub fn start() -> impl Future<Item = (), Error = ()> {
    let size = termion::terminal_size().unwrap_or((0, 0));
    let mut render_buffer = Vec::with_capacity(size.0 as usize * size.1 as usize + 1);
    let mut out = io::stdout().into_raw_mode().unwrap();

    write!(render_buffer, "{}", cursor::Hide).unwrap();

    let mut content: Box<Component> = Box::new(panels::LoginPanel::new());
    let mut logged_in = false;

    let mut interval = timer::Interval::new(Instant::now(), Duration::from_secs(1));

    // Hack to avoid having to reaquire lock in each loop iteration
    let mut notifications = mem::replace(&mut *NOTIFICATIONS.1.lock(), None).unwrap();

    future::poll_fn(move || {
        let mut render = interval.poll().map_err(|_| ())?.is_ready();

        while let Async::Ready(Some(not)) = notifications.poll()? {
            match not {
                Notify::Login => {
                    let height = termion::terminal_size().unwrap_or((0, 0)).1;
                    content = Box::new(panels::MainPanel::new(height));
                    logged_in = true;
                    render = true;
                }
                // RPC was closed, since we can't differentiate between regular closal and
                // errors in the RPC future, we use this workaround
                Notify::Close if logged_in => {
                    content = Box::new(panels::LoginPanel::new());
                    logged_in = false;
                    render = true;
                }
                Notify::Close => {
                    return Err(());
                }
                Notify::Rpc(msg) => {
                    content.rpc(msg);
                    render = true;
                }
                Notify::Input(Key::Ctrl('q')) if logged_in => {
                    #[cfg(feature = "dbg")]
                    debug!(*::S_VIEW, "Disconnecting");

                    // `logged_in` is set to false in Task::Close for simplicity
                    rpc::disconnect();
                    render = true;
                }
                Notify::Input(Key::Ctrl('q')) => {
                    #[cfg(feature = "dbg")]
                    debug!(*::S_VIEW, "Closing");

                    return Ok(Async::Ready(()));
                }
                Notify::Input(key) => {
                    let s = termion::terminal_size().unwrap_or((0, 0));
                    match content.input(key, s.0, s.1) {
                        InputResult::ReplaceWith(other) => {
                            mem::replace(&mut content, other);
                            render = true;
                        }
                        InputResult::Rerender => {
                            render = true;
                        }
                        _ => (),
                    }
                }
                Notify::Overlay(name, text, color) => {
                    // Because we can't move the component out, we need to use this hack
                    // This is only safe because we leak ct below, so that it won't get freed
                    let alias_cur = unsafe { Box::from_raw((&mut *content) as *mut Component) };

                    let len = cmp::max(text.len(), name.len()) + 2;
                    let overlay = Box::new(widgets::OwnedOverlay::new(
                        widgets::CloseOnInput::new(
                            widgets::IgnoreRpc::new(widgets::Text::<
                                _,
                                align::x::Center,
                                align::y::Top,
                            >::new(true, text)),
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
                    mem::forget(mem::replace(&mut content, overlay));
                    render = true;
                }
            }
        }

        if render {
            if let Ok((width, height)) = termion::terminal_size() {
                write!(render_buffer, "{}", clear::All).unwrap();
                content.render(&mut render_buffer, width, height, 1, 1);

                out.write_all(&*render_buffer).unwrap();
                out.flush().unwrap();
                render_buffer.clear();
            } else {
                write!(out, "smol").unwrap();
                out.flush().unwrap();
            }
        }

        Ok(Async::NotReady)
    })
    .then(|_| -> Result<(), ()> {
        #[cfg(feature = "dbg")]
        debug!(*::S_VIEW, "View finishing");

        // Unhide the cursor
        let _ = write!(io::stdout(), "{}", cursor::Show);

        process::exit(0)
    })
}
