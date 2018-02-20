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

pub mod tui;

use parking_lot::{Condvar, Mutex};
use synapse_rpc::message::SMessage;
use termion::{self, clear, cursor};
use termion::event::Key;
use termion::raw::{IntoRawMode, RawTerminal};
use websocket;

use std::cell::RefCell;
use std::io::{self, Stdout, Write};
use std::mem::{self, ManuallyDrop};
use std::sync::atomic::Ordering;
use std::time::Duration;

use rpc::RpcContext;
use self::tui::{widgets, Component, InputResult, Renderable};
use utils::align;

enum DisplayState {
    GlobalErr(String, Option<String>, Box<Component>),
    Component(Box<Component>),
}

pub struct View {
    content: Mutex<DisplayState>,
    render_buf: Mutex<Vec<u8>>,
    waiter: (Condvar, Mutex<()>),
    stdout: RefCell<RawTerminal<Stdout>>,
    // Unfortunately can't check with Any::is if the component is the login panel
    logged_in: RefCell<bool>,
}

unsafe impl Send for View {}
unsafe impl Sync for View {}

impl View {
    pub fn init() -> View {
        let panel = Box::new(widgets::IgnoreRpcPassInput::new(tui::LoginPanel::new()));

        let size = termion::terminal_size().unwrap_or((0, 0));
        let mut rb = Vec::with_capacity(size.0 as usize * size.1 as usize + 1);

        write!(rb, "{}", cursor::Hide).unwrap();
        View {
            content: Mutex::new(DisplayState::Component(panel)),
            render_buf: Mutex::new(rb),
            stdout: RefCell::new(io::stdout().into_raw_mode().unwrap()),
            waiter: (Condvar::new(), Mutex::new(())),
            logged_in: RefCell::new(false),
        }
    }

    pub fn wake(&self) {
        self.waiter.0.notify_one();
    }

    pub fn render(&self) {
        let mut cnt = self.content.lock();
        if let Ok((width, height)) = termion::terminal_size() {
            let mut buf = self.render_buf.lock();
            write!(buf, "{}", clear::All).unwrap();

            match *cnt {
                DisplayState::Component(ref mut cmp) => {
                    cmp.render(&mut buf, width, height, 1, 1);
                }
                DisplayState::GlobalErr(ref err, ref err_name, ref mut cmp) => {
                    widgets::BorrowedOverlay::new(
                        &mut widgets::Text::<_, align::x::Center, align::y::Top>::new(true, &**err),
                        &mut **cmp,
                        (err.len() as u16 + 2, 1),
                        Some(&termion::color::Red),
                        err_name.as_ref().map(|o| &o[..]),
                    ).render(&mut buf, width, height, 1, 1);
                }
            }

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
        while ::RUNNING.load(Ordering::Acquire) {
            // Update either every 5s or when input demands it
            self.render();
            self.waiter
                .0
                .wait_for(&mut self.waiter.1.lock(), Duration::from_secs(5));
        }
    }

    pub fn handle_input(&self, ctx: &RpcContext, k: Key) -> InputResult {
        match k {
            Key::Ctrl('d') => if !*self.logged_in.borrow() {
                InputResult::Close
            } else {
                ctx.wake();
                self.server_close(None);
                InputResult::Rerender
            },
            Key::F(5) => InputResult::Rerender,
            _ => {
                let mut cnt = self.content.lock();

                // FIXME: NLL
                let was_err = if let DisplayState::GlobalErr(_, _, _) = *cnt {
                    true
                } else {
                    false
                };
                let new = match *cnt {
                    DisplayState::GlobalErr(_, _, ref mut cmp)
                    | DisplayState::Component(ref mut cmp) => DisplayState::Component(unsafe {
                        Box::from_raw((&mut **cmp) as *mut Component)
                    }),
                };
                ManuallyDrop::new(mem::replace(&mut *cnt, new));
                if was_err {
                    InputResult::Rerender
                } else {
                    // Simulate CloseOnInput
                    let ret = match *cnt {
                        DisplayState::GlobalErr(_, _, ref mut cmp)
                        | DisplayState::Component(ref mut cmp) => cmp.input(ctx, k),
                    };
                    match ret {
                        InputResult::ReplaceWith(comp) => {
                            let mut li = self.logged_in.borrow_mut();
                            if !*li {
                                *li = true;
                            }
                            *cnt = DisplayState::Component(comp);
                            InputResult::Rerender
                        }
                        _ => ret,
                    }
                }
            }
        }
    }

    pub fn handle_rpc(&self, ctx: &RpcContext, msg: &SMessage) {
        // FIXME: NLL
        let mut cnt = self.content.lock();
        if match *cnt {
            DisplayState::GlobalErr(_, _, ref mut cmp) | DisplayState::Component(ref mut cmp) => {
                cmp.rpc(ctx, msg)
            }
        } {
            drop(cnt);
            self.wake();
        }
    }

    pub fn global_err<T, U>(&self, err: T, err_name: Option<U>)
    where
        T: ::std::fmt::Display,
        U: ::std::fmt::Display,
    {
        // FIXME: NLL
        let mut cnt = self.content.lock();
        let new = match *cnt {
            DisplayState::GlobalErr(_, _, ref mut cmp) | DisplayState::Component(ref mut cmp) => {
                DisplayState::GlobalErr(
                    format!("{}", err),
                    err_name.map(|e| format!("{}", e)),
                    unsafe { Box::from_raw((&mut **cmp) as *mut Component) },
                )
            }
        };
        ManuallyDrop::new(mem::replace(&mut *cnt, new));
    }

    pub fn server_close(&self, data: Option<websocket::CloseData>) {
        let mut cnt = self.content.lock();
        *self.logged_in.borrow_mut() = false;
        let msg = data.map(|d| format!("{:?}", d))
            .unwrap_or_else(|| "Disconnected".to_owned());
        mem::replace(
            &mut *cnt,
            DisplayState::GlobalErr(
                msg,
                Some("Server closed".to_owned()),
                Box::new(widgets::IgnoreRpcPassInput::new(tui::LoginPanel::new())),
            ),
        );
    }
}
