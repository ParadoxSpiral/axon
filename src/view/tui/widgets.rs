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

use synapse_rpc::message::SMessage;
use termion::color::Color;
use termion::event::Key;
use termion::{color, cursor, style};
use unicode_segmentation::UnicodeSegmentation;

use std::borrow::{Borrow, BorrowMut};
use std::io::Write;
use std::marker::PhantomData;
use std::mem::{self, ManuallyDrop};
use std::str;

use rpc::RpcContext;
use utils;
use utils::align::{self, x, y};
use super::{Component, HandleInput, HandleRpc, InputResult, Renderable};

pub struct VSplit<'a, L: 'a, R: 'a>
where
    L: BorrowMut<Renderable + 'a>,
    R: BorrowMut<Renderable + 'a>,
{
    left: L,
    right: R,
    left_active: bool,
    left_size_factor: f32,
    _marker: PhantomData<&'a ()>,
}

impl<'a, L, R> VSplit<'a, L, R>
where
    L: BorrowMut<Renderable + 'a>,
    R: BorrowMut<Renderable + 'a>,
{
    pub fn new(left: L, right: R, left_active: bool, left_size_factor: f32) -> VSplit<'a, L, R> {
        assert!(left_size_factor < 1. && left_size_factor > 0.);
        VSplit {
            left: left,
            right: right,
            left_active: left_active,
            left_size_factor: left_size_factor,
            _marker: PhantomData,
        }
    }
}

impl<'a, L, R> Renderable for VSplit<'a, L, R>
where
    L: BorrowMut<Renderable + 'a>,
    R: BorrowMut<Renderable + 'a>,
{
    fn name(&self) -> String {
        format!(
            "({} ╍ {})",
            self.left.borrow().name(),
            self.right.borrow().name()
        )
    }
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        // Draw left
        let left_w = (f32::from(width) * self.left_size_factor).floor() as u16;
        self.left
            .borrow_mut()
            .render(target, left_w, height, x_off, y_off);

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
        self.right.borrow_mut().render(
            target,
            width - left_w - 1,
            height,
            x_off + left_w + 2,
            y_off,
        );
    }
}

pub struct HSplit<'a, T: 'a, B: 'a>
where
    T: BorrowMut<Renderable + 'a>,
    B: BorrowMut<Renderable + 'a>,
{
    top: T,
    bot: B,
    top_active: bool,
    top_size_factor: f32,
    _marker: PhantomData<&'a ()>,
}

impl<'a, T: 'a, B: 'a> HSplit<'a, T, B>
where
    T: BorrowMut<Renderable + 'a>,
    B: BorrowMut<Renderable + 'a>,
{
    pub fn new(top: T, bot: B, top_active: bool, top_size_factor: f32) -> HSplit<'a, T, B> {
        HSplit {
            top: top,
            bot: bot,
            top_active: top_active,
            top_size_factor: top_size_factor,
            _marker: PhantomData,
        }
    }
}

impl<'a, T: 'a, B: 'a> Renderable for HSplit<'a, T, B>
where
    T: BorrowMut<Renderable + 'a>,
    B: BorrowMut<Renderable + 'a>,
{
    fn name(&self) -> String {
        format!(
            "({} ╏ {})",
            self.top.borrow().name(),
            self.bot.borrow().name()
        )
    }
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        // Draw top
        let top_h = (f32::from(height) * self.top_size_factor).floor() as u16;
        self.top
            .borrow_mut()
            .render(target, width, top_h, x_off, y_off);

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
            .borrow_mut()
            .render(target, width, height - top_h - 1, x_off, y_off + top_h + 2);
    }
}

pub struct Tabs {
    tabs: Vec<Box<Component>>,
    active_idx: usize,
}
impl Tabs {
    pub fn new(tabs: Vec<Box<Component>>, active: usize) -> Tabs {
        assert!(active < tabs.len() || active == 0);
        Tabs {
            tabs: tabs,
            active_idx: active,
        }
    }

    pub fn push(&mut self, tab: Box<Component>) {
        self.tabs.push(tab);
    }
    pub fn n_tabs(&self) -> usize {
        self.tabs.len()
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
            Text::<_, align::x::Left, align::y::Top>::new(name).render(
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
        if !self.tabs.is_empty() {
            self.tabs.get_mut(self.active_idx).unwrap().rpc(ctx, msg);
        }
    }
    fn init(&mut self, ctx: &RpcContext) {
        for t in &mut self.tabs {
            t.init(ctx);
        }
    }
}

pub struct BorrowedOverlay<'a, T: 'a, B: 'a, C: 'a>
where
    T: Renderable + ?Sized,
    B: Renderable + ?Sized,
    C: Color,
{
    top: &'a mut T,
    below: &'a mut B,
    top_dimensions: (u16, u16),
    box_color: Option<&'a C>,
}

impl<'a, T, B, C> BorrowedOverlay<'a, T, B, C>
where
    T: Renderable + ?Sized,
    B: Component + ?Sized,
    C: Color,
{
    pub fn new(
        top: &'a mut T,
        below: &'a mut B,
        top_dimensions: (u16, u16),
        box_color: Option<&'a C>,
    ) -> BorrowedOverlay<'a, T, B, C> {
        assert!(top_dimensions.0 > 0 && top_dimensions.1 > 0);
        BorrowedOverlay {
            top: top,
            below: below,
            top_dimensions: top_dimensions,
            box_color: box_color,
        }
    }
}

impl<'a, T, B, C> Renderable for BorrowedOverlay<'a, T, B, C>
where
    T: Renderable + ?Sized,
    B: Component + ?Sized,
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
        let (start_color, end_color) = if let Some(c) = self.box_color {
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

pub struct OwnedOverlay<T, C>
where
    T: Component,
    C: Color,
{
    top: T,
    below: ManuallyDrop<Box<Component>>,
    top_dimensions: (u16, u16),
    box_color: Option<C>,
}
impl<T, C> OwnedOverlay<T, C>
where
    T: Component,
    C: Color,
{
    pub fn new<I: Into<Option<C>>>(
        top: T,
        below: Box<Component>,
        top_dimensions: (u16, u16),
        box_color: I,
    ) -> OwnedOverlay<T, C> {
        assert!(top_dimensions.0 > 0 && top_dimensions.1 > 0);
        OwnedOverlay {
            top: top,
            below: ManuallyDrop::new(below),
            top_dimensions: top_dimensions,
            box_color: box_color.into(),
        }
    }
}

impl<T, C> Component for OwnedOverlay<T, C>
where
    T: Component,
    C: Color,
{
}

impl<T, C> Renderable for OwnedOverlay<T, C>
where
    T: Component,
    C: Color,
{
    fn name(&self) -> String {
        format!("overlay: {}^_{}", self.top.name(), self.below.name())
    }
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        BorrowedOverlay::<_, _, C>::new(
            &mut self.top,
            &mut **self.below,
            self.top_dimensions,
            self.box_color.as_ref(),
        ).render(target, width, height, x_off, y_off)
    }
}

impl<T, C> HandleRpc for OwnedOverlay<T, C>
where
    T: Component,
    C: Color,
{
    fn init(&mut self, ctx: &RpcContext) {
        self.top.init(ctx);
        self.below.init(ctx);
    }
    fn rpc(&mut self, ctx: &RpcContext, msg: &SMessage) {
        self.top.rpc(ctx, msg);
        self.below.rpc(ctx, msg);
    }
}

impl<T, C> HandleInput for OwnedOverlay<T, C>
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

pub struct Text<T, AX, AY>
where
    T: Borrow<str>,
    AX: x::Align,
    AY: y::Align,
{
    content: T,
    _align_x: PhantomData<AX>,
    _align_y: PhantomData<AY>,
}

impl<T, AX, AY> Text<T, AX, AY>
where
    T: Borrow<str>,
    AX: x::Align,
    AY: y::Align,
{
    pub fn new(t: T) -> Text<T, AX, AY> {
        Text {
            content: t,
            _align_x: PhantomData,
            _align_y: PhantomData,
        }
    }
}

impl<T, AX, AY> Renderable for Text<T, AX, AY>
where
    T: Borrow<str>,
    AX: x::Align,
    AY: y::Align,
{
    fn name(&self) -> String {
        "txt".into()
    }
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        let content = self.content.borrow();
        let x_off = x_off + match AX::align_offset(&[content], width) {
            x::Alignment::Single(x) => x,
            x::Alignment::Each(v) => {
                // TODO: unimpl for n > 1
                assert_eq!(1, v.len());
                *v.first().unwrap()
            }
        };
        let y_off = y_off + AY::align_offset(&[content], height);
        let len = utils::count_without_styling(content);

        if width >= len as u16 {
            write!(target, "{}{}", cursor::Goto(x_off, y_off), content).unwrap();
        } else {
            let mut chunks = content
                .graphemes(true)
                // Version of .chunks that preserves control codes
                .fold((0, 0, false, Vec::new(), vec![String::new()]), |mut acc, g| {
                    // idx, crr_cnt, inside_esc, esc, str
                    // FIXME: see utils::count_without_styling
                    if g == "\x1B" {
                        acc.2 = true;
                        acc.3.push(String::from("\x1B"));
                        acc.4[acc.0].push('\x1B');
                    } else if acc.2 && g != "m" {
                        acc.3[acc.0].push_str(g);
                        acc.4[acc.0].push_str(g);
                    } else if acc.2 && g == "m" {
                        acc.2 = false;
                        let mut r_idx = 0;
                        if acc.3[acc.0] == "\x1B[m" {
                            // Reset styling
                            acc.3.reverse();
                            for (i, c) in acc.3.iter().skip(1).enumerate() {
                                if !c.contains(';') {
                                    r_idx = acc.3.len() - 2 - i;
                                    break;
                                }
                            }
                        } else if acc.3[acc.0] == "\x1B[39m" {
                            // Reset fg color
                            acc.3.reverse();
                            for (i, c) in acc.3.iter().skip(1).enumerate() {
                                if c.starts_with("38;") {
                                    r_idx = acc.3.len() - 2 - i;
                                    break;
                                }
                            }
                        } else if acc.3[acc.0] == "\x1B[49m" {
                            // Reset bg color
                            acc.3.reverse();
                            for (i, c) in acc.3.iter().skip(1).enumerate() {
                                if c.starts_with("48;") {
                                    r_idx = acc.3.len() - 2 - i;
                                    break;
                                }
                            }
                        } else {
                            acc.3[acc.0].push('m');
                            acc.4[acc.0].push('m');
                            return acc;
                        }
                        acc.3.reverse();
                        let l = acc.3.len();
                        acc.3.remove(l - 1);
                        acc.3.remove(r_idx);
                    } else {
                        let l = utils::count(g);
                        if acc.1 + l >= width as _ {
                            assert!(!acc.2);
                            acc.3.reverse();
                            acc.4.push(String::new());
                            for esc in &acc.3 {
                                if esc.starts_with("\x1B[38;") {
                                    acc.4[acc.0].push_str(&format!("{}", color::Fg(color::Reset)));
                                } else if esc.starts_with("\x1B[48;") {
                                    acc.4[acc.0].push_str(&format!("{}", color::Bg(color::Reset)));
                                } else {
                                    acc.4[acc.0].push_str(&format!("{}", style::Reset));
                                }
                                acc.4[acc.0+1].push_str(&esc);
                            }
                            acc.3.reverse();
                            acc.0 +=1;
                            acc.1 = l;
                            acc.4[acc.0].push_str(g);
                        } else {
                            acc.1 += l;
                            acc.4[acc.0].push_str(g);
                        }
                    }
                    acc
                }).4.into_iter().filter(|s| utils::count_without_styling(s) != 0).peekable();
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
                .and_then(|pos| {
                    assert!(pos > 0);
                    Some(pos)
                })
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
    pub fn clear(&mut self) {
        self.content.clear();
        self.pos = 1;
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
            // FIXME: allocs less than ideal
            format!(
                "{}{}{}{}{}",
                &self.content
                    .graphemes(true)
                    .take(self.pos - 1)
                    .collect::<String>(),
                style::Underline,
                if self.pos > 1 || !self.content.is_empty() {
                    self.content
                        .graphemes(true)
                        .skip(self.pos - 1)
                        .take(1)
                        .collect::<String>()
                } else {
                    " ".into()
                },
                style::NoUnderline,
                if self.pos + 1 <= len {
                    self.content
                        .graphemes(true)
                        .skip(self.pos)
                        .collect::<String>()
                } else {
                    "".into()
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

pub struct RenderFn<F>
where
    F: Fn(&mut Vec<u8>, u16, u16, u16, u16),
{
    ct: F,
}
impl<F> RenderFn<F>
where
    F: Fn(&mut Vec<u8>, u16, u16, u16, u16),
{
    pub fn new(fun: F) -> RenderFn<F> {
        RenderFn { ct: fun }
    }
}
impl<F> Renderable for RenderFn<F>
where
    F: Fn(&mut Vec<u8>, u16, u16, u16, u16),
{
    fn name(&self) -> String {
        "Unnamed render fun".into()
    }
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        (self.ct)(target, width, height, x_off, y_off);
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
    fn init(&mut self, ctx: &RpcContext) {
        self.content.init(ctx);
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
    fn init(&mut self, _: &RpcContext) {}
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
    fn init(&mut self, _: &RpcContext) {}
}
