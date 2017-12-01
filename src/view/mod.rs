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

use parking_lot::Mutex;
use synapse_rpc::message::SMessage;
use termion::{self, clear, cursor};
use termion::event::Key;

use std::cell::RefCell;
use std::io::{Stdout, StdoutLock, Write};
use std::mem::{self, ManuallyDrop};

use rpc::RpcContext;
use self::tui::{widgets, InputResult, Renderable};

pub struct View<'a> {
    content: Mutex<Box<tui::Component>>,
    global_err: RefCell<Option<String>>,
    render_buf: Mutex<Vec<u8>>,
    stdout: RefCell<StdoutLock<'a>>,
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
            content: Mutex::new(panel),
            global_err: RefCell::new(None),
            render_buf: Mutex::new(rb),
            stdout: RefCell::new(stdout.lock()),
        }
    }

    pub fn render(&self) {
        let mut cnt = self.content.lock();
        if let Ok((width, height)) = termion::terminal_size() {
            let mut buf = self.render_buf.lock();
            write!(buf, "{}", clear::All).unwrap();

            if let Some(ref err) = *self.global_err.borrow() {
                let mut ov = widgets::Overlay::new(
                    widgets::CloseOnInput::new(widgets::IgnoreRpc::new(
                        widgets::BorrowedText::<::utils::align::Center>::new(err),
                    )),
                    unsafe {
                        Box::from_raw((&mut **cnt) as *const tui::Component as *mut tui::Component)
                    },
                    (err.len() as u16 + 2, 1),
                    termion::color::Red,
                );
                ov.render(&mut buf, width, height, 1, 1);
                mem::forget(ov.into_below());
            } else {
                cnt.render(&mut buf, width, height, 1, 1);
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

    pub fn handle_input(&self, ctx: &RpcContext, k: Key) -> InputResult {
        match k {
            Key::Ctrl('d') => InputResult::Close,
            _ => {
                let mut cnt = self.content.lock();
                *self.global_err.borrow_mut() = None;
                let ret = cnt.input(ctx, k);
                match ret {
                    InputResult::ReplaceWith(comp, drop) => {
                        if drop {
                            let _ = mem::replace(&mut *cnt, comp);
                        } else {
                            ManuallyDrop::new(mem::replace(&mut *cnt, comp));
                        }
                        InputResult::Rerender
                    }
                    _ => ret,
                }
            }
        }
    }

    pub fn handle_rpc(&self, ctx: &RpcContext, msg: &SMessage) {
        self.content.lock().rpc(ctx, msg);
    }

    pub fn global_err<T>(&self, err: T)
    where
        T: ::std::fmt::Display,
    {
        let cnt = self.content.lock();
        *self.global_err.borrow_mut() = Some(format!("{}", err));
    }
}
