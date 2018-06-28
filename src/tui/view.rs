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

use parking_lot::{Condvar, Mutex};
use synapse_rpc::message::SMessage;
use termion::event::Key;
use termion::raw::{IntoRawMode, RawTerminal};
use termion::{self, clear, color, cursor};
use websocket;

use std::cell::RefCell;
use std::cmp;
use std::io::{self, Stdout, Write};
use std::mem;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::{panels, widgets, Component, InputResult};
use rpc::RpcContext;
use utils::align;

pub struct View {
    content: Mutex<Box<Component>>,
    render_buf: Mutex<Vec<u8>>,
    waiter: (Condvar, Mutex<()>),
    running: AtomicBool,
    logged_in: AtomicBool,
    stdout: RefCell<RawTerminal<Stdout>>,
}

unsafe impl Send for View {}
unsafe impl Sync for View {}

impl View {
    pub fn init() -> View {
        let size = termion::terminal_size().unwrap_or((0, 0));
        let mut rb = Vec::with_capacity(size.0 as usize * size.1 as usize + 1);

        write!(rb, "{}", cursor::Hide).unwrap();
        View {
            content: Mutex::new(Box::new(panels::LoginPanel::new())),
            render_buf: Mutex::new(rb),
            stdout: RefCell::new(io::stdout().into_raw_mode().unwrap()),
            running: AtomicBool::new(true),
            logged_in: AtomicBool::new(false),
            waiter: (Condvar::new(), Mutex::new(())),
        }
    }

    pub fn logged_in(&self) -> bool {
        self.logged_in.load(Ordering::Acquire)
    }

    // Called by RPC to signify successful login
    pub fn login(&self, rpc: &RpcContext) {
        let mut ct = self.content.lock();
        *ct = Box::new(panels::MainPanel::new(rpc));
        self.logged_in.store(true, Ordering::Release);
    }

    pub fn wake(&self) {
        self.waiter.0.notify_one();
    }

    pub fn shutdown(&self) {
        self.running.store(false, Ordering::Release);
        self.wake();
    }

    pub fn render_until_death(&self) {
        while self.running.load(Ordering::Acquire) {
            // Render either every s or when input demands it
            self.render();
            self.waiter
                .0
                .wait_for(&mut self.waiter.1.lock(), Duration::from_secs(1));
        }
    }

    pub fn render(&self) {
        let mut ct = self.content.lock();
        if let Ok((width, height)) = termion::terminal_size() {
            let mut buf = self.render_buf.lock();
            write!(buf, "{}", clear::All).unwrap();

            ct.render(&mut buf, width, height, 1, 1);

            let mut o = self.stdout.borrow_mut();
            o.write_all(&*buf).unwrap();
            o.flush().unwrap();
            buf.clear();
        } else {
            let mut o = self.stdout.borrow_mut();
            write!(o, "smol").unwrap();
            o.flush().unwrap();
        }
    }

    pub fn handle_input(&self, ctx: &RpcContext, k: Key) -> InputResult {
        match k {
            Key::Ctrl('q') => if self.logged_in() {
                #[cfg(feature = "dbg")]
                debug!(*::S_VIEW, "Disconnecting");

                ctx.disconnect();
                self.connection_close(None);

                InputResult::Rerender
            } else {
                #[cfg(feature = "dbg")]
                debug!(*::S_VIEW, "Closing");

                ctx.disconnect();
                self.shutdown();

                InputResult::Close
            },
            _ => {
                let s = termion::terminal_size().unwrap_or((0, 0));
                let mut ct = self.content.lock();
                match ct.input(ctx, k, s.0, s.1) {
                    InputResult::ReplaceWith(other) => {
                        mem::replace(&mut *ct, other);
                        InputResult::Rerender
                    }
                    res => res,
                }
            }
        }
    }

    pub fn handle_rpc(&self, ctx: &RpcContext, msg: SMessage) {
        if self.content.lock().rpc(ctx, msg) {
            self.wake();
        }
    }

    pub fn overlay<C: color::Color + 'static>(&self, name: String, text: String, color: Option<C>) {
        let mut ct = self.content.lock();
        // Because we can't move the component out, we need to use this hack
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
        drop(ct);
        self.wake();
    }

    pub fn connection_close(&self, data: Option<websocket::CloseData>) {
        let mut ct = self.content.lock();
        self.logged_in.store(false, Ordering::Release);

        *ct = Box::new(panels::LoginPanel::new());
        drop(ct);
        self.overlay(
            "Connection closed".to_owned(),
            data.map(|d| format!("{}", d.reason))
                .unwrap_or_else(|| "Disconnected".to_owned()),
            Some(color::Red),
        );
    }
}
