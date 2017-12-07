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

use itertools::Itertools;
use synapse_rpc::message::SMessage;
use termion::color::Color;
use termion::event::Key;
use termion::{color, cursor, style};
use unicode_segmentation::UnicodeSegmentation;

use std::io::Write;
use std::marker::PhantomData;
use std::mem::{self, ManuallyDrop};
use std::str;

use rpc::RpcContext;
use utils;
use utils::align::{Align, Alignment, Left};
use super::{Component, HandleInput, HandleRpc, InputResult, Renderable};

pub struct VSplit {
    left: ManuallyDrop<Box<Component>>,
    right: ManuallyDrop<Box<Component>>,
    left_active: bool,
    left_size_factor: f32,
}

impl VSplit {
    pub fn new<L: 'static, R: 'static>(
        left: L,
        right: R,
        left_active: bool,
        left_size_factor: f32,
    ) -> VSplit
    where
        L: Component,
        R: Component,
    {
        VSplit {
            left: ManuallyDrop::new(Box::new(left)),
            right: ManuallyDrop::new(Box::new(right)),
            left_active: left_active,
            left_size_factor: left_size_factor,
        }
    }
}

impl Drop for VSplit {
    fn drop(&mut self) {
        unsafe {
            if self.left_active {
                ManuallyDrop::drop(&mut self.left);
            } else {
                ManuallyDrop::drop(&mut self.right);
            }
        }
    }
}

impl Component for VSplit {}

impl Renderable for VSplit {
    fn name(&self) -> String {
        format!("({} ╍ {})", self.left.name(), self.right.name())
    }
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        // Draw left
        let left_w = (f32::from(width) * self.left_size_factor).floor() as u16;
        self.left.render(target, left_w, height, x_off, y_off);

        // Draw divider
        for i in 0..height {
            write!(
                target,
                "{}{}",
                cursor::Goto(x_off + left_w + 1, y_off + i),
                {
                    if self.left_active && i < height / 2 || !self.left_active && i > height / 2 {
                        format!("{}┃{}", color::Fg(color::Cyan), color::Fg(color::Reset))
                    } else {
                        "│".into()
                    }
                }
            ).unwrap();
        }

        // Draw right
        self.right.render(
            target,
            width - left_w - 1,
            height,
            x_off + left_w + 2,
            y_off,
        );
    }
}

impl HandleInput for VSplit {
    fn input(&mut self, ctx: &RpcContext, k: Key) -> InputResult {
        match if self.left_active {
            self.left.input(ctx, k)
        } else {
            self.right.input(ctx, k)
        } {
            InputResult::Key(Key::Char('h')) => if self.left_active {
                InputResult::Key(Key::Char('h'))
            } else {
                self.left_active = true;
                InputResult::Rerender
            },
            InputResult::Key(Key::Char('l')) => if self.left_active {
                self.left_active = false;
                InputResult::Rerender
            } else {
                InputResult::Key(Key::Char('l'))
            },
            InputResult::ReplaceWith(cmp) => {
                if self.left_active {
                    let _ = mem::replace(&mut *self.left, cmp);
                } else {
                    let _ = mem::replace(&mut *self.right, cmp);
                }

                InputResult::Rerender
            }
            InputResult::Close => if self.left_active {
                InputResult::ReplaceWith(unsafe {
                    Box::from_raw((&**self.right) as *const Component as *mut Component)
                })
            } else {
                InputResult::ReplaceWith(unsafe {
                    Box::from_raw((&**self.left) as *const Component as *mut Component)
                })
            },
            ret => ret,
        }
    }
}

impl HandleRpc for VSplit {
    fn rpc(&mut self, ctx: &RpcContext, msg: &SMessage) {
        self.left.rpc(ctx, msg);
        self.right.rpc(ctx, msg);
    }
}

pub struct HSplit {
    top: ManuallyDrop<Box<Component>>,
    bot: ManuallyDrop<Box<Component>>,
    top_active: bool,
    top_size_factor: f32,
}

impl Drop for HSplit {
    fn drop(&mut self) {
        unsafe {
            if self.top_active {
                ManuallyDrop::drop(&mut self.top);
            } else {
                ManuallyDrop::drop(&mut self.bot);
            }
        }
    }
}

impl HSplit {
    pub fn new<T: 'static, B: 'static>(
        top: T,
        bot: B,
        top_active: bool,
        top_size_factor: f32,
    ) -> HSplit
    where
        T: Component,
        B: Component,
    {
        HSplit {
            top: ManuallyDrop::new(Box::new(top)),
            bot: ManuallyDrop::new(Box::new(bot)),
            top_active: top_active,
            top_size_factor: top_size_factor,
        }
    }
}

impl Component for HSplit {}

impl Renderable for HSplit {
    fn name(&self) -> String {
        format!("({} ╏ {})", self.top.name(), self.bot.name())
    }
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        // Draw top
        let top_h = (f32::from(height) * self.top_size_factor).floor() as u16;
        self.top.render(target, width, top_h, x_off, y_off);

        // Draw divider
        for i in 0..width {
            write!(
                target,
                "{}{}",
                cursor::Goto(x_off + i, y_off + top_h + 1),
                {
                    if self.top_active && i < width / 2 || !self.top_active && i > width / 2 {
                        format!("{}━{}", color::Fg(color::Cyan), color::Fg(color::Reset))
                    } else {
                        "─".into()
                    }
                }
            ).unwrap();
        }

        // Draw bot
        self.bot
            .render(target, width, height - top_h - 1, x_off, y_off + top_h + 2);
    }
}

impl HandleInput for HSplit {
    fn input(&mut self, ctx: &RpcContext, k: Key) -> InputResult {
        match if self.top_active {
            self.top.input(ctx, k)
        } else {
            self.bot.input(ctx, k)
        } {
            InputResult::Key(Key::Char('k')) => if self.top_active {
                InputResult::Key(Key::Char('k'))
            } else {
                self.top_active = true;
                InputResult::Rerender
            },
            InputResult::Key(Key::Char('j')) => if self.top_active {
                self.top_active = false;
                InputResult::Rerender
            } else {
                InputResult::Key(Key::Char('j'))
            },
            InputResult::ReplaceWith(cmp) => {
                if self.top_active {
                    let _ = mem::replace(&mut *self.top, cmp);
                } else {
                    let _ = mem::replace(&mut *self.bot, cmp);
                }
                InputResult::Rerender
            }
            InputResult::Close => if self.top_active {
                InputResult::ReplaceWith(unsafe {
                    Box::from_raw((&**self.bot) as *const Component as *mut Component)
                })
            } else {
                InputResult::ReplaceWith(unsafe {
                    Box::from_raw((&**self.top) as *const Component as *mut Component)
                })
            },
            ret => ret,
        }
    }
}

impl HandleRpc for HSplit {
    fn rpc(&mut self, ctx: &RpcContext, msg: &SMessage) {
        self.top.rpc(ctx, msg);
        self.bot.rpc(ctx, msg);
    }
}

pub struct Tabs {
    tabs: Vec<Box<Component>>,
    active_idx: usize,
}
impl Tabs {
    pub fn new(tabs: Vec<Box<Component>>, active: usize) -> Tabs {
        assert!(tabs.len() > 1);
        assert!(active < tabs.len());
        Tabs {
            tabs: tabs,
            active_idx: active,
        }
    }
}

impl Component for Tabs {}

impl Renderable for Tabs {
    fn name(&self) -> String {
        if self.tabs.len() == 1 {
            format!("{}", self.tabs.first().unwrap().name())
        } else {
            let mut names = String::new();
            for (i, c) in self.tabs.iter().enumerate() {
                if i > 0 {
                    names.push_str(" | ");
                }
                names.push_str(&c.name());
            }
            format!("tabs: {}", names)
        }
    }
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        // Draw header
        let len = self.tabs.len() as u16;
        let n_tabs = self.tabs.len();
        for (i, t) in self.tabs.iter().enumerate() {
            let name = t.name();
            let name_l = utils::count_without_styling(&name) as u16;
            let mut x_off = x_off + i as u16 * (width / len);
            let mut compensate = false;
            let sep = if width / len < name_l {
                "".into()
            } else {
                // Compensate if width is uneven
                if i + 1 == n_tabs && (f32::from(width) / f32::from(len)) % 2. != 0. {
                    compensate = true;
                    // Overwrite last elem of previous sep, or there will be a gap
                    if x_off != 1 {
                        x_off -= 1;
                    }
                }
                (0..(width / len - name_l) / 2).fold("".to_owned(), |acc, _| acc + "─")
            };
            let sep_l = utils::count_without_styling(&sep) as u16;

            write!(target, "{}{}", cursor::Goto(x_off, y_off), sep).unwrap();
            if self.active_idx == i {
                write!(target, "{}", color::Fg(color::Cyan)).unwrap();
            }
            BorrowedText::<Left>::new(&name).render(
                target,
                width / len - sep_l,
                1,
                x_off + sep_l,
                y_off,
            );
            if self.active_idx == i {
                write!(target, "{}", color::Fg(color::Reset)).unwrap();
            }
            if compensate {
                write!(target, "{}──", sep).unwrap();
            } else {
                write!(target, "{}", sep).unwrap();
            }
        }

        // Draw active component
        self.tabs.get_mut(self.active_idx).unwrap().render(
            target,
            width,
            height - 1,
            x_off,
            y_off + 1,
        );
    }
}

impl HandleInput for Tabs {
    fn input(&mut self, ctx: &RpcContext, k: Key) -> InputResult {
        let len = self.tabs.len();

        match self.tabs.get_mut(self.active_idx).unwrap().input(ctx, k) {
            InputResult::Key(Key::Char('l')) => if self.active_idx + 1 < len {
                self.active_idx += 1;
                InputResult::Rerender
            } else {
                InputResult::Key(Key::Char('l'))
            },
            InputResult::Key(Key::Char('h')) => if self.active_idx > 0 {
                self.active_idx -= 1;
                InputResult::Rerender
            } else {
                InputResult::Key(Key::Char('h'))
            },
            InputResult::Close => if len == 2 {
                InputResult::ReplaceWith(if self.active_idx == 0 {
                    self.tabs.remove(1)
                } else {
                    self.tabs.remove(0)
                })
            } else {
                if self.active_idx == len - 1 {
                    self.active_idx -= 1;
                }
                self.tabs.remove(self.active_idx);
                InputResult::Rerender
            },
            InputResult::ReplaceWith(cmp) => {
                let _ = mem::replace(&mut *self.tabs.get_mut(self.active_idx).unwrap(), cmp);
                InputResult::Rerender
            }
            ret => ret,
        }
    }
}

impl HandleRpc for Tabs {
    fn rpc(&mut self, ctx: &RpcContext, msg: &SMessage) {
        self.tabs.get_mut(self.active_idx).unwrap().rpc(ctx, msg);
    }
}

pub struct Overlay<T, C>
where
    T: Component,
    C: Color,
{
    top: T,
    below: ManuallyDrop<Box<Component>>,
    top_dimensions: (u16, u16),
    box_color: Option<C>,
}

impl<T, C> Overlay<T, C>
where
    T: Component,
    C: Color,
{
    pub fn new<I: Into<Option<C>>>(
        top: T,
        below: Box<Component>,
        top_dimensions: (u16, u16),
        box_color: I,
    ) -> Overlay<T, C> {
        assert!(top_dimensions.0 > 0 && top_dimensions.1 > 0);
        Overlay {
            top: top,
            below: ManuallyDrop::new(below),
            top_dimensions: top_dimensions,
            box_color: box_color.into(),
        }
    }
    pub fn into_below(self) -> Box<Component> {
        ManuallyDrop::into_inner(self.below)
    }
}

impl<T, C> Component for Overlay<T, C>
where
    T: Component,
    C: Color,
{
}

impl<T, C> Renderable for Overlay<T, C>
where
    T: Component,
    C: Color,
{
    fn name(&self) -> String {
        format!("overlay: {}^_{}", self.top.name(), self.below.name())
    }
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        // Render lower layer
        self.below.render(target, width, height, x_off, y_off);

        let x_off = x_off + (width / 2).saturating_sub(self.top_dimensions.0 / 2 + 1);
        let y_off = y_off + (height / 2).saturating_sub(self.top_dimensions.1 / 2 + 1);

        // Prepare writing the overlay box
        let delim_hor = (0..self.top_dimensions.0).fold("".to_owned(), |s, _| s + "─");
        let (start_color, end_color) = if let Some(ref c) = self.box_color {
            (
                format!("{}", color::Fg(c as &Color)),
                format!("{}", color::Fg(color::Reset)),
            )
        } else {
            (
                format!("{}{}", color::Fg(color::Black), color::Fg(color::Reset)),
                "".into(),
            )
        };

        // Write box around top layer
        write!(
            target,
            "{}{}┌{}┐{}",
            cursor::Goto(x_off, y_off),
            start_color,
            delim_hor,
            end_color,
        ).unwrap();
        for i in 1..(self.top_dimensions.1 + 1) {
            write!(
                target,
                "{}{}│{}{}{}│{}",
                cursor::Goto(x_off, y_off + i),
                start_color,
                end_color,
                cursor::Goto(x_off + 1 + self.top_dimensions.0, y_off + i),
                start_color,
                end_color
            ).unwrap();
        }
        write!(
            target,
            "{}{}└{}┘{}",
            cursor::Goto(x_off, y_off + self.top_dimensions.1 + 1),
            start_color,
            delim_hor,
            end_color
        ).unwrap();

        // Write top layer, hope that the render doesn't violate the bounds
        self.top.render(
            target,
            self.top_dimensions.0,
            self.top_dimensions.1,
            x_off + 1,
            y_off + 1,
        );
    }
}

impl<T, C> HandleInput for Overlay<T, C>
where
    T: Component,
    C: Color,
{
    fn input(&mut self, ctx: &RpcContext, k: Key) -> InputResult {
        let ret = self.top.input(ctx, k);
        match ret {
            InputResult::Close => InputResult::ReplaceWith(unsafe {
                Box::from_raw((&mut **self.below) as *mut Component)
            }),
            _ => ret,
        }
    }
}

impl<T, C> HandleRpc for Overlay<T, C>
where
    T: Component,
    C: Color,
{
    fn rpc(&mut self, ctx: &RpcContext, msg: &SMessage) {
        self.top.rpc(ctx, msg);
        self.below.rpc(ctx, msg);
    }
}

pub struct List<T>
where
    T: ::std::fmt::Display,
{
    list: Vec<T>,
}

pub struct BorrowedText<'a, A>
where
    A: Align,
{
    content: &'a str,
    _align: PhantomData<A>,
}

impl<'a, A> BorrowedText<'a, A>
where
    A: Align,
{
    pub fn new(t: &str) -> BorrowedText<A> {
        BorrowedText {
            content: t,
            _align: PhantomData,
        }
    }
}

impl<'a, A> Renderable for BorrowedText<'a, A>
where
    A: Align,
{
    fn name(&self) -> String {
        "txt".into()
    }
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        let x_off = x_off + match A::align_offset(&[self.content], width) {
            Alignment::Single(x) => x,
            Alignment::Each(v) => {
                debug_assert_eq!(1, v.len());
                *v.first().unwrap()
            }
        };
        let len = utils::count_without_styling(self.content);

        if width >= len as u16 {
            write!(target, "{}{}", cursor::Goto(x_off, y_off), self.content).unwrap();
        } else {
            // FIXME: don't break up escape sequences
            let chunks = self.content.graphemes(true).chunks((width - 1) as usize);
            let mut chunks = chunks.into_iter().map(|c| c.collect::<String>()).peekable();
            let mut i = 0;
            while let Some(chunk) = chunks.next() {
                if let Some(n_chunk) = chunks.peek() {
                    if utils::count_without_styling(n_chunk) > 1 {
                        if i + 1 >= height {
                            // Truncate
                            write!(target, "{}{}…", cursor::Goto(x_off, y_off + i), chunk)
                                .unwrap();
                            break;
                        } else {
                            // Wrap
                            write!(target, "{}{}-", cursor::Goto(x_off, y_off + i), chunk).unwrap();
                        }
                        i += 1;
                    } else {
                        // Next chunk is small and the last chunk, needs no new line
                        write!(
                            target,
                            "{}{}{}",
                            cursor::Goto(x_off, y_off + 1),
                            chunk,
                            n_chunk
                        ).unwrap();
                        break;
                    }
                } else {
                    // Last chunk
                    write!(target, "{}{}", cursor::Goto(x_off, y_off + 1), chunk).unwrap();
                    break;
                }
            }
        }
    }
}

pub struct OwnedText<A>
where
    A: Align,
{
    content: String,
    _align: PhantomData<A>,
}

impl<A> OwnedText<A>
where
    A: Align,
{
    pub fn new(t: String) -> OwnedText<A> {
        OwnedText {
            content: t,
            _align: PhantomData,
        }
    }
}

impl<A> Renderable for OwnedText<A>
where
    A: Align,
{
    fn name(&self) -> String {
        "txt".into()
    }
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        BorrowedText::<A>::new(&self.content).render(target, width, height, x_off, y_off);
    }
}

#[derive(Clone)]
pub struct Input {
    content: String,
    pos: usize,
}

impl Input {
    pub fn from<T: Into<Option<usize>>>(str: &str, pos: T) -> Input {
        Input {
            content: String::from(str),
            pos: pos.into()
                .unwrap_or_else(|| str.graphemes(true).count() + 1),
        }
    }
    pub fn with_capacity(n: usize) -> Input {
        Input {
            content: String::with_capacity(n),
            pos: 1,
        }
    }
    pub fn inner(&self) -> &str {
        &self.content
    }

    pub fn cursor_left(&mut self) {
        if self.pos > 1 {
            self.pos -= 1;
        }
    }
    pub fn cursor_right(&mut self) {
        if self.pos <= self.content.graphemes(true).count() {
            self.pos += 1;
        }
    }
    pub fn push(&mut self, c: char) {
        self.content.insert(self.pos - 1, c);
        self.pos += 1;
    }
    pub fn backspace(&mut self) {
        if self.pos > 1 {
            self.content = self.content
                .graphemes(true)
                .take(self.pos - 2)
                .chain(self.content.graphemes(true).skip(self.pos - 2).skip(1))
                .collect();
            self.pos -= 1;
        }
    }
    pub fn delete(&mut self) {
        if !self.content.is_empty() && self.pos <= self.content.graphemes(true).count() {
            self.content = self.content
                .graphemes(true)
                .take(self.pos - 1)
                .chain(self.content.graphemes(true).skip(self.pos - 1).skip(1))
                .collect();
        }
    }

    pub fn format_active(&mut self) -> String {
        let len = self.content.graphemes(true).count();
        if self.pos > len {
            format!(
                "{}{} {}",
                &self.content,
                style::Underline,
                style::NoUnderline,
            )
        } else {
            format!(
                "{}{}{}{}{}",
                &self.content[..self.pos - 1],
                style::Underline,
                if self.pos > 1 || !self.content.is_empty() {
                    &self.content[self.pos - 1..self.pos]
                } else {
                    " "
                },
                style::NoUnderline,
                if self.pos + 1 <= len {
                    &self.content[self.pos..]
                } else {
                    ""
                }
            )
        }
    }
    pub fn format_inactive(&self) -> &str {
        &self.content
    }
}

#[derive(Clone)]
pub struct PasswordInput(Input);
impl PasswordInput {
    pub fn with_capacity(n: usize) -> PasswordInput {
        PasswordInput(Input::with_capacity(n))
    }
}
impl ::std::ops::Deref for PasswordInput {
    type Target = Input;
    fn deref(&self) -> &Input {
        &self.0
    }
}
impl ::std::ops::DerefMut for PasswordInput {
    fn deref_mut(&mut self) -> &mut Input {
        &mut self.0
    }
}
impl PasswordInput {
    pub fn format_active(&mut self) -> String {
        let len = self.content.graphemes(true).count();
        let stars = (0..len).fold("".to_owned(), |s, _| s + "*");
        if self.pos > len {
            format!("{}{} {}", &stars, style::Underline, style::NoUnderline,)
        } else {
            format!(
                "{}{}{}{}{}",
                &stars[..self.pos - 1],
                style::Underline,
                if self.pos > 1 || !self.content.is_empty() {
                    &stars[self.pos - 1..self.pos]
                } else {
                    " "
                },
                style::NoUnderline,
                if self.pos + 1 <= len {
                    &stars[self.pos..]
                } else {
                    ""
                }
            )
        }
    }
    pub fn format_inactive(&self) -> String {
        (0..self.content.graphemes(true).count()).fold("".to_owned(), |s, _| s + "*")
    }
}

pub struct CloseOnInput<T>
where
    T: Renderable + HandleRpc,
{
    content: T,
}

impl<T> CloseOnInput<T>
where
    T: Renderable + HandleRpc,
{
    pub fn new(ct: T) -> CloseOnInput<T> {
        CloseOnInput { content: ct }
    }
}

impl<T> Component for CloseOnInput<T>
where
    T: Renderable + HandleRpc,
{
}

impl<T> Renderable for CloseOnInput<T>
where
    T: Renderable + HandleRpc,
{
    fn name(&self) -> String {
        format!("close on input: {}", self.content.name())
    }
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        self.content.render(target, width, height, x_off, y_off);
    }
}

impl<T> HandleInput for CloseOnInput<T>
where
    T: Renderable + HandleRpc,
{
    fn input(&mut self, _: &RpcContext, _: Key) -> InputResult {
        InputResult::Close
    }
}

impl<T> HandleRpc for CloseOnInput<T>
where
    T: Renderable + HandleRpc,
{
    fn rpc(&mut self, ctx: &RpcContext, msg: &SMessage) {
        self.content.rpc(ctx, msg);
    }
}

pub struct IgnoreRpc<T>
where
    T: Renderable,
{
    content: T,
}

impl<T> IgnoreRpc<T>
where
    T: Renderable,
{
    pub fn new(ct: T) -> IgnoreRpc<T> {
        IgnoreRpc { content: ct }
    }
}

impl<T> Renderable for IgnoreRpc<T>
where
    T: Renderable,
{
    fn name(&self) -> String {
        self.content.name()
    }
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        self.content.render(target, width, height, x_off, y_off);
    }
}

impl<T> HandleRpc for IgnoreRpc<T>
where
    T: Renderable,
{
    fn rpc(&mut self, _: &RpcContext, _: &SMessage) {}
}

pub struct IgnoreRpcPassInput<T>
where
    T: Renderable + HandleInput,
{
    content: T,
}

impl<T> IgnoreRpcPassInput<T>
where
    T: Renderable + HandleInput,
{
    pub fn new(ct: T) -> IgnoreRpcPassInput<T> {
        IgnoreRpcPassInput { content: ct }
    }
}

impl<T> Component for IgnoreRpcPassInput<T>
where
    T: Renderable + HandleInput,
{
}

impl<T> Renderable for IgnoreRpcPassInput<T>
where
    T: Renderable + HandleInput,
{
    fn name(&self) -> String {
        self.content.name()
    }
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        self.content.render(target, width, height, x_off, y_off);
    }
}

impl<T> HandleInput for IgnoreRpcPassInput<T>
where
    T: Renderable + HandleInput,
{
    fn input(&mut self, ctx: &RpcContext, k: Key) -> InputResult {
        self.content.input(ctx, k)
    }
}

impl<T> HandleRpc for IgnoreRpcPassInput<T>
where
    T: Renderable + HandleInput,
{
    fn rpc(&mut self, _: &RpcContext, _: &SMessage) {}
}
