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
use termion::{self, clear, cursor};
use websocket;

use std::cell::RefCell;
use std::cmp;
use std::fmt::Display;
use std::io::{self, Stdout, Write};
use std::mem;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::{panels, widgets, Component, HandleInput, HandleRpc, InputResult, Renderable};
use rpc::RpcContext;
use utils::align;

pub struct View {
    content: Mutex<DisplayState>,
    render_buf: Mutex<Vec<u8>>,
    waiter: (Condvar, Mutex<()>),
    running: AtomicBool,
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
            content: Mutex::new(DisplayState::Component(TopLevelComponent::Login(
                panels::LoginPanel::new(),
            ))),
            render_buf: Mutex::new(rb),
            stdout: RefCell::new(io::stdout().into_raw_mode().unwrap()),
            running: AtomicBool::new(true),
            waiter: (Condvar::new(), Mutex::new(())),
        }
    }

    pub fn wake(&self) {
        self.waiter.0.notify_one();
    }

    pub fn shutdown(&self) {
        self.running.store(false, Ordering::Release);
        self.wake();
    }

    pub fn render(&self) {
        let mut cnt = self.content.lock();
        if let Ok((width, height)) = termion::terminal_size() {
            let mut buf = self.render_buf.lock();
            write!(buf, "{}", clear::All).unwrap();

            cnt.render(&mut buf, width, height);

            let mut o = self.stdout.borrow_mut();
            o.write_all(&*buf).unwrap();
            o.flush().unwrap();
            buf.clear();
        } else {
            let mut o = self.stdout.borrow_mut();
            write!(o, "small!").unwrap();
            o.flush().unwrap();
        }
    }

    pub fn render_until_death(&self) {
        while self.running.load(Ordering::Acquire) {
            // Render  either every 5s or when input demands it
            self.render();
            self.waiter
                .0
                .wait_for(&mut self.waiter.1.lock(), Duration::from_secs(5));
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
                self.content.lock().input(ctx, k, s.0, s.1)
            }
        }
    }

    pub fn handle_rpc(&self, ctx: &RpcContext, msg: SMessage) {
        if self.content.lock().rpc(ctx, msg) {
            self.wake();
        }
    }

    pub fn global_err<T: Display, U: Display>(&self, err: T, name: Option<U>) {
        self.content
            .lock()
            .global_err(format!("{}", err), name.map(|n| format!("{}", n)));
        self.wake();
    }

    pub fn connection_close(&self, data: Option<websocket::CloseData>) {
        *self.content.lock() = DisplayState::GlobalErr(
            data.map(|d| format!("{}", d.reason))
                .unwrap_or_else(|| "Disconnected".to_owned()),
            Some("Connection closed".to_owned()),
            TopLevelComponent::Login(panels::LoginPanel::new()),
        );
    }

    pub fn logged_in(&self) -> bool {
        match *self.content.lock() {
            DisplayState::Component(TopLevelComponent::Login(_))
            | DisplayState::GlobalErr(_, _, TopLevelComponent::Login(_)) => false,
            _ => true,
        }
    }

    // Called by RPC to signify successful login
    pub fn login(&self, rpc: &RpcContext) {
        let mut ct = self.content.lock();
        *ct = DisplayState::Component(TopLevelComponent::Other(Box::new(panels::MainPanel::new(
            rpc,
        ))));
    }
}

enum DisplayState {
    Component(TopLevelComponent),
    GlobalErr(String, Option<String>, TopLevelComponent),
}

impl DisplayState {
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16) {
        match *self {
            DisplayState::Component(ref mut cmp) => {
                cmp.render(target, width, height, 1, 1);
            }
            DisplayState::GlobalErr(ref err, ref name, ref mut cmp) => {
                widgets::BorrowedOverlay::new(
                    &mut widgets::Text::<_, align::x::Center, align::y::Top>::new(true, &**err),
                    &mut *cmp,
                    (
                        cmp::max(err.len(), name.as_ref().map(|n| n.len()).unwrap_or(0)) as u16 + 2,
                        1,
                    ),
                    Some(&termion::color::Red),
                    name.as_ref().map(|o| &o[..]),
                ).render(target, width, height, 1, 1);
            }
        }
    }
    fn input(&mut self, ctx: &RpcContext, k: Key, width: u16, height: u16) -> InputResult {
        // FIXME: Shitty borrow-checker pleaser
        if let DisplayState::Component(ref mut cmp) = *self {
            return cmp.input(ctx, k, width, height);
        }
        if [Key::Esc, Key::Backspace, Key::Delete, Key::Char('q')].contains(&k) {
            let (clone, drop) = if let DisplayState::GlobalErr(_, _, ref mut cmp) = *self {
                match *cmp {
                    TopLevelComponent::Login(ref l) => (
                        DisplayState::Component(TopLevelComponent::Login(l.clone())),
                        true,
                    ),
                    TopLevelComponent::Other(ref mut o) => (
                        DisplayState::Component(TopLevelComponent::Other(unsafe {
                            Box::from_raw((&mut **o) as *mut Component)
                        })),
                        false,
                    ),
                }
            } else {
                unreachable!()
            };
            if drop {
                mem::replace(self, clone);
            } else {
                mem::forget(mem::replace(self, clone));
            }
        }
        InputResult::Rerender
    }
    fn rpc(&mut self, ctx: &RpcContext, msg: SMessage) -> bool {
        match *self {
            DisplayState::Component(TopLevelComponent::Other(ref mut cmp))
            | DisplayState::GlobalErr(_, _, TopLevelComponent::Other(ref mut cmp)) => {
                cmp.rpc(ctx, msg)
            }
            _ => false,
        }
    }
    fn global_err(&mut self, err: String, name: Option<String>) {
        // FIXME: Shitty borrow-checker pleaser
        if let DisplayState::GlobalErr(ref mut e_err, ref mut e_name, _) = *self {
            *e_err = err;
            *e_name = name;
            return;
        }
        let clone = DisplayState::GlobalErr(
            err,
            name,
            if let DisplayState::Component(ref mut cmp) = *self {
                TopLevelComponent::Other(unsafe { Box::from_raw(cmp as *mut Component) })
            } else {
                unreachable!()
            },
        );
        mem::forget(mem::replace(self, clone));
    }
}

enum TopLevelComponent {
    Login(panels::LoginPanel),
    Other(Box<Component>),
}

impl Component for TopLevelComponent {}

impl Renderable for TopLevelComponent {
    fn name(&self) -> String {
        match *self {
            TopLevelComponent::Login(ref l) => l.name(),
            TopLevelComponent::Other(ref o) => o.name(),
        }
    }
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        match *self {
            TopLevelComponent::Login(ref mut l) => l.render(target, width, height, x_off, y_off),
            TopLevelComponent::Other(ref mut o) => o.render(target, width, height, x_off, y_off),
        }
    }
}
impl HandleInput for TopLevelComponent {
    fn input(&mut self, ctx: &RpcContext, k: Key, width: u16, height: u16) -> InputResult {
        // We know that LoginPanel will only return a MainPanel with ReplaceWith, any errors are
        // handled internally for simplicity
        match match *self {
            TopLevelComponent::Login(ref mut l) => l.input(ctx, k, width, height),
            TopLevelComponent::Other(ref mut o) => o.input(ctx, k, width, height),
        } {
            InputResult::ReplaceWith(cmp) => {
                mem::replace(self, TopLevelComponent::Other(cmp));
                InputResult::Rerender
            }
            ir => ir,
        }
    }
}
impl HandleRpc for TopLevelComponent {
    fn rpc(&mut self, ctx: &RpcContext, msg: SMessage) -> bool {
        match *self {
            TopLevelComponent::Other(ref mut o) => o.rpc(ctx, msg),
            _ => false,
        }
    }
    fn init(&mut self, _: &RpcContext) {
        unreachable!()
    }
}
