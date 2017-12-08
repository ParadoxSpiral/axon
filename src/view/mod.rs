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
use websocket;

use std::cell::RefCell;
use std::io::{Stdout, StdoutLock, Write};
use std::mem::{self, ManuallyDrop};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rpc::RpcContext;
use self::tui::{widgets, Component, InputResult, Renderable};
use utils::align;

enum DisplayState {
    GlobalErr(String, Box<Component>),
    Component(Box<Component>),
}

pub struct View<'a> {
    content: Mutex<DisplayState>,
    render_buf: Mutex<Vec<u8>>,
    stdout: RefCell<StdoutLock<'a>>,
    waiter: (Condvar, Mutex<()>),
    // Unfortunately can't check with Any::is if the component is the login panel
    logged_in: RefCell<bool>,
}

unsafe impl<'a> Send for View<'a> {}
unsafe impl<'a> Sync for View<'a> {}

// Restore cursor
impl<'a> Drop for View<'a> {
    fn drop(&mut self) {
        let mut stdo = self.stdout.borrow_mut();
        write!(stdo, "{}", cursor::Show).unwrap();
        stdo.flush().unwrap();
    }
}

impl<'a> View<'a> {
    pub fn init(stdout: &'a Stdout) -> View<'a> {
        let panel = Box::new(widgets::IgnoreRpcPassInput::new(tui::LoginPanel::new()));

        let size = termion::terminal_size().unwrap_or((0, 0));
        let mut rb = Vec::with_capacity(size.0 as usize * size.1 as usize + 1);

        write!(rb, "{}", cursor::Hide).unwrap();
        View {
            content: Mutex::new(DisplayState::Component(panel)),
            render_buf: Mutex::new(rb),
            stdout: RefCell::new(stdout.lock()),
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
                DisplayState::GlobalErr(ref err, ref mut cmp) => {
                    widgets::BorrowedOverlay::new(
                        &mut widgets::BorrowedText::<align::x::Center, align::y::Top>::new(err),
                        &mut **cmp,
                        (err.len() as u16 + 2, 1),
                        Some(&termion::color::Red),
                    ).render(&mut buf, width, height, 1, 1);
                }
            }

            let mut out = self.stdout.borrow_mut();
            out.write_all(&*buf).unwrap();
            out.flush().unwrap();
            buf.clear();
        } else {
            let mut stdout = self.stdout.borrow_mut();
            write!(stdout, "small!").unwrap();
            stdout.flush().unwrap();
        }
    }

    pub fn render_until_death(&self, running: &AtomicBool) {
        while running.load(Ordering::Acquire) {
            // Update either every 3s or when input demands it
            self.render();
            self.waiter
                .0
                .wait_for(&mut self.waiter.1.lock(), Duration::from_millis(2500));
        }
    }

    pub fn handle_input(&self, ctx: &RpcContext, k: Key) -> InputResult {
        match k {
            Key::Ctrl('d') => if !*self.logged_in.borrow() {
                InputResult::Close
            } else {
                ctx.server_close();
                self.server_close(None);
                InputResult::Rerender
            },
            _ => {
                let mut cnt = self.content.lock();

                // FIXME: NLL
                let was_err = if let DisplayState::GlobalErr(_, _) = *cnt {
                    true
                } else {
                    false
                };
                let new = match *cnt {
                    DisplayState::GlobalErr(_, ref mut cmp)
                    | DisplayState::Component(ref mut cmp) => DisplayState::Component(
                        unsafe { Box::from_raw((&mut **cmp) as *mut Component) },
                    ),
                };
                ManuallyDrop::new(mem::replace(&mut *cnt, new));
                // Simulate CloseOnInput
                if was_err {
                    InputResult::Rerender
                } else {
                    let ret = match *cnt {
                        DisplayState::GlobalErr(_, ref mut cmp)
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
        match *cnt {
            DisplayState::GlobalErr(_, ref mut cmp) | DisplayState::Component(ref mut cmp) => {
                cmp.rpc(ctx, msg)
            }
        }
    }

    pub fn global_err<T>(&self, err: T)
    where
        T: ::std::fmt::Display,
    {
        // FIXME: NLL
        let mut cnt = self.content.lock();
        let new = match *cnt {
            DisplayState::GlobalErr(_, ref mut cmp) | DisplayState::Component(ref mut cmp) => {
                DisplayState::GlobalErr(format!("{}", err), unsafe {
                    Box::from_raw((&mut **cmp) as *mut Component)
                })
            }
        };
        ManuallyDrop::new(mem::replace(&mut *cnt, new));
    }

    pub fn server_close(&self, data: Option<websocket::CloseData>) {
        let mut cnt = self.content.lock();
        *self.logged_in.borrow_mut() = false;
        let msg = data.and_then(|data| Some(format!("Server closed, reason: {:?}", data)))
            .unwrap_or_else(|| "Disconnected".to_owned());
        mem::replace(
            &mut *cnt,
            DisplayState::GlobalErr(
                msg,
                Box::new(widgets::IgnoreRpcPassInput::new(tui::LoginPanel::new())),
            ),
        );
    }
}
