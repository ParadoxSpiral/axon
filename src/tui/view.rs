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
use termion::{self, clear, color, cursor};
use tokio::prelude::*;
use tokio::timer;

use std::cmp;
use std::io::{self, Write};
use std::mem;
use std::time::{Duration, Instant};

use super::{panels, widgets, Component, InputResult};
use rpc;
use utils::align;

lazy_static! {
    static ref RPC: (
        Mutex<mpsc::UnboundedSender<SMessage<'static>>>,
        Mutex<Option<mpsc::UnboundedReceiver<SMessage<'static>>>>
    ) = {
        let (s, r) = mpsc::unbounded();
        (Mutex::new(s), Mutex::new(Some(r)))
    };
    static ref INPUT: (
        Mutex<mpsc::UnboundedSender<Key>>,
        Mutex<Option<mpsc::UnboundedReceiver<Key>>>
    ) = {
        let (s, r) = mpsc::unbounded();
        (Mutex::new(s), Mutex::new(Some(r)))
    };
    static ref TASKS: (
        Mutex<mpsc::UnboundedSender<Task>>,
        Mutex<Option<mpsc::UnboundedReceiver<Task>>>
    ) = {
        let (s, r) = mpsc::unbounded();
        (Mutex::new(s), Mutex::new(Some(r)))
    };
}

pub enum Task {
    Login,
    // name, text, color
    Overlay(String, String, Option<Box<dyn color::Color + Send + Sync>>),
    Close,
}

// We don't care about the errors, because it only signifies closal, which implies shutdown
#[allow(unused_must_use)]
impl Task {
    pub fn login() {
        TASKS.0.lock().unbounded_send(Task::Login);
    }
    pub fn close() {
        TASKS.0.lock().unbounded_send(Task::Close);
    }
    pub fn overlay(name: String, text: String, color: Option<Box<dyn color::Color + Send + Sync>>) {
        TASKS
            .0
            .lock()
            .unbounded_send(Task::Overlay(name, text, color));
    }
}

pub fn notify_rpc(msg: SMessage<'static>) {
    let _ = RPC.0.lock().unbounded_send(msg);
}

pub fn notify_input(k: Key) {
    let _ = INPUT.0.lock().unbounded_send(k);
}

macro_rules! handle_ipc {
    ($queue:ident, $fun:expr) => {
        loop {
            match $queue.poll() {
                Err(_) => {
                    rpc::disconnect();
                    return Err(());
                }
                Ok(Async::Ready(Some(msg))) => {
                    // Return early, can't do this directly because closures
                    if $fun(msg) {
                        return Ok(Async::Ready(()));
                    }
                }
                // Not ready, or stream finished
                _ => {
                    break;
                }
            }
        }
    };
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
    let mut rpc = None;
    let mut input = None;
    let mut tasks = None;
    mem::swap(&mut *RPC.1.lock(), &mut rpc);
    mem::swap(&mut *INPUT.1.lock(), &mut input);
    mem::swap(&mut *TASKS.1.lock(), &mut tasks);
    let mut rpc = rpc.unwrap();
    let mut input = input.unwrap();
    let mut tasks = tasks.unwrap();

    future::poll_fn(move || {
        let mut render = false;

        handle_ipc!(rpc, |msg| {
            content.rpc(msg);
            render = true;
            false
        });

        handle_ipc!(input, |key| {
            let res = match key {
                Key::Ctrl('q') => {
                    if logged_in {
                        #[cfg(feature = "dbg")]
                        debug!(*::S_VIEW, "Disconnecting");

                        // `logged_in` is set to false in Task::Close for simplicity
                        rpc::disconnect();

                        InputResult::Rerender
                    } else {
                        #[cfg(feature = "dbg")]
                        debug!(*::S_VIEW, "Closing");

                        InputResult::Close
                    }
                }
                _ => {
                    let s = termion::terminal_size().unwrap_or((0, 0));
                    match content.input(key, s.0, s.1) {
                        InputResult::ReplaceWith(other) => {
                            mem::replace(&mut content, other);
                            InputResult::Rerender
                        }
                        res => res,
                    }
                }
            };
            match res {
                InputResult::Rerender => {
                    render = true;
                }
                InputResult::Close => {
                    rpc.close();
                    input.close();
                    tasks.close();
                    return true;
                }
                _ => (),
            }

            false
        });

        handle_ipc!(tasks, |task| {
            match task {
                Task::Login => {
                    content = Box::new(panels::MainPanel::new());
                    logged_in = true;
                    render = true;
                }
                Task::Close => {
                    // RPC was closed, since we can't differentiate between regular closal and
                    // errors in the RPC future, we use this workaround
                    if logged_in {
                        content = Box::new(panels::LoginPanel::new());
                        logged_in = false;
                        render = true;
                    } else {
                        // There was an error
                        return true;
                    }
                }
                Task::Overlay(name, text, color) => {
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

            false
        });

        if !render {
            handle_ipc!(interval, |_| {
                render = true;
                false
            });
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
    .then(move |v| {
        #[cfg(feature = "dbg")]
        debug!(*::S_VIEW, "View finishing");

        // Unhide the cursor
        write!(io::stdout(), "{}", cursor::Show);

        v
    })
}
