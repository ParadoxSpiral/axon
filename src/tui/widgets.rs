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
use termion::{cursor, event::Key, style};
use unicode_segmentation::UnicodeSegmentation;

use std::{
    borrow::{Borrow, BorrowMut},
    io::Write,
    marker::PhantomData,
    mem::ManuallyDrop,
    ops::Range,
    str,
};

use crate::{
    tui::{Component, HandleInput, HandleRpc, InputResult, Renderable},
    utils::{
        self,
        align::{self, x, y},
        color::ColorEscape,
    },
};

// TODO: Splitup in windowing, content, and util widgets

pub enum Unit {
    Lines(u16),
    Percent(f32),
}

pub struct VSplit<'a, L: 'a, R: 'a>
where
    L: BorrowMut<Renderable + 'a> + Send,
    R: BorrowMut<Renderable + 'a> + Send,
{
    left: L,
    right: R,
    left_active: Option<bool>,
    left_size: Unit,
    draw_div: bool,
    _marker: PhantomData<&'a ()>,
}

impl<'a, L, R> VSplit<'a, L, R>
where
    L: BorrowMut<Renderable + 'a> + Send,
    R: BorrowMut<Renderable + 'a> + Send,
{
    pub fn new(
        left: L,
        right: R,
        left_active: Option<bool>,
        left_size: Unit,
        draw_div: bool,
    ) -> VSplit<'a, L, R> {
        VSplit {
            left,
            right,
            left_active,
            left_size,
            draw_div,
            _marker: PhantomData,
        }
    }
}

impl<'a, L, R> Renderable for VSplit<'a, L, R>
where
    L: BorrowMut<Renderable + 'a> + Send,
    R: BorrowMut<Renderable + 'a> + Send,
{
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        // Draw left
        let left_w = match self.left_size {
            Unit::Lines(w) => w,
            Unit::Percent(p) => (f32::from(width) * p).floor() as u16,
        };
        self.left
            .borrow_mut()
            .render(target, left_w, height, x_off, y_off);

        let comp = if self.draw_div {
            // Draw divider
            for i in 0..height {
                write!(target, "{}{}", cursor::Goto(x_off + left_w, y_off + i), {
                    if self.left_active.unwrap_or(false) && i < height / 2
                        || !self.left_active.unwrap_or(true) && i > height / 2
                    {
                        format!("{}│{}", ColorEscape::cyan(), ColorEscape::reset())
                    } else {
                        "│".into()
                    }
                })
                .unwrap();
            }
            1
        } else {
            0
        };

        // Draw right
        self.right.borrow_mut().render(
            target,
            width - left_w - comp,
            height,
            x_off + left_w + comp,
            y_off,
        );
    }
}

pub struct HSplit<'a, T: 'a, B: 'a>
where
    T: BorrowMut<Renderable + 'a> + Send,
    B: BorrowMut<Renderable + 'a> + Send,
{
    top: T,
    bot: B,
    top_active: Option<bool>,
    top_size: Unit,
    draw_div: bool,
    _marker: PhantomData<&'a ()>,
}

impl<'a, T: 'a, B: 'a> HSplit<'a, T, B>
where
    T: BorrowMut<Renderable + 'a> + Send,
    B: BorrowMut<Renderable + 'a> + Send,
{
    pub fn new(
        top: T,
        bot: B,
        top_active: Option<bool>,
        top_size: Unit,
        draw_div: bool,
    ) -> HSplit<'a, T, B> {
        HSplit {
            top,
            bot,
            top_active,
            top_size,
            draw_div,
            _marker: PhantomData,
        }
    }
}

impl<'a, T: 'a, B: 'a> Renderable for HSplit<'a, T, B>
where
    T: BorrowMut<Renderable + 'a> + Send,
    B: BorrowMut<Renderable + 'a> + Send,
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
        let top_h = match self.top_size {
            Unit::Lines(h) => h,
            Unit::Percent(p) => (f32::from(height) * p).floor() as u16,
        };
        self.top
            .borrow_mut()
            .render(target, width, top_h, x_off, y_off);

        let comp = if self.draw_div {
            // Draw divider
            let div = (0..width).fold("".to_owned(), |acc, i| {
                if self.top_active.unwrap_or(false) && i == 0
                    || !self.top_active.unwrap_or(true) && i == width / 2
                {
                    acc + &*format!("{}─", ColorEscape::cyan())
                } else if self.top_active.unwrap_or(false) && i == width / 2
                    || !self.top_active.unwrap_or(true) && i == width
                {
                    acc + &*format!("─{}", ColorEscape::reset())
                } else {
                    acc + "─"
                }
            });
            write!(target, "{}{}", cursor::Goto(x_off, y_off + top_h), div).unwrap();
            1
        } else {
            0
        };

        // Draw bot
        self.bot.borrow_mut().render(
            target,
            width,
            height - top_h - comp,
            x_off,
            y_off + top_h + comp,
        );
    }
}

pub struct BorrowedSameTabs<'a, T: 'a>
where
    T: Renderable + 'a + Send,
{
    tabs: &'a mut [T],
    active_idx: usize,
}
impl<'a, T> BorrowedSameTabs<'a, T>
where
    T: Renderable + 'a + Send,
{
    pub fn new(tabs: &'a mut [T], active: usize) -> BorrowedSameTabs<'a, T> {
        BorrowedSameTabs {
            tabs,
            active_idx: active,
        }
    }
}

impl<'a, T> Renderable for BorrowedSameTabs<'a, T>
where
    T: Renderable + 'a + Send,
{
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        write!(target, "{}", cursor::Goto(x_off, y_off)).unwrap();

        // Draw header
        let n_tabs = self.tabs.len();
        let sec_len = width / n_tabs as u16;
        let div_budget = "─".repeat(width.saturating_sub(self.tabs.iter().fold(0, |acc, t| {
            acc + utils::count_without_styling(&t.borrow().name())
        })) as usize);
        let mut div_budget = div_budget.chars();
        let div_budget = div_budget.by_ref();
        for (i, t) in self.tabs.iter_mut().enumerate() {
            let t = (*t).borrow_mut();
            let div_len = sec_len.saturating_sub(utils::count_without_styling(&t.name())) / 2;
            write!(
                target,
                "{}{}",
                if self.active_idx == i {
                    format!("{}", ColorEscape::cyan())
                } else {
                    "".to_owned()
                },
                div_budget.take(div_len as usize + 1).collect::<String>(),
            )
            .unwrap();
            Text::<_, align::x::Left, align::y::Top>::new(false, t.name()).render(
                target,
                // FIXME: Width too small if content truncated
                sec_len,
                1,
                x_off + i as u16 * sec_len + div_len + 1,
                y_off,
            );
            write!(
                target,
                "{}{}",
                if i + 1 == n_tabs {
                    div_budget.as_str().to_owned()
                } else {
                    div_budget.take(div_len as usize + 1).collect()
                },
                if self.active_idx == i {
                    ColorEscape::reset()
                } else {
                    ColorEscape::empty()
                },
            )
            .unwrap();
        }

        // Draw active component
        self.tabs[self.active_idx]
            .borrow_mut()
            .render(target, width, height - 1, x_off, y_off + 1);
    }
}

pub struct BorrowedOverlay<'a, T: 'a, B: 'a>
where
    T: Renderable + ?Sized + Send,
    B: Renderable + ?Sized + Send,
{
    top: &'a mut T,
    below: &'a mut B,
    top_dimensions: (u16, u16),
    box_color: Option<&'a ColorEscape>,
    name: Option<&'a str>,
}

impl<'a, T, B> BorrowedOverlay<'a, T, B>
where
    T: Renderable + ?Sized + Send,
    B: Renderable + ?Sized + Send,
{
    pub fn new<J: Into<Option<&'a str>>>(
        top: &'a mut T,
        below: &'a mut B,
        top_dimensions: (u16, u16),
        box_color: Option<&'a ColorEscape>,
        name: J,
    ) -> BorrowedOverlay<'a, T, B> {
        BorrowedOverlay {
            top,
            below,
            top_dimensions,
            box_color,
            name: name.into(),
        }
    }
}

impl<'a, T, B> Renderable for BorrowedOverlay<'a, T, B>
where
    T: Renderable + ?Sized + Send,
    B: Renderable + ?Sized + Send,
{
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        // Render lower layer
        self.below.render(target, width, height, x_off, y_off);

        let x_off = x_off + (width / 2).saturating_sub(self.top_dimensions.0 / 2 + 1);
        let y_off = y_off + (height / 2).saturating_sub(self.top_dimensions.1 / 2 + 1);

        // Prepare writing the overlay box
        let delim_hor = "─".repeat(self.top_dimensions.0 as _);
        let (c_s, c_e) = if let Some(c) = self.box_color {
            (format!("{}", c), ColorEscape::reset())
        } else {
            (
                format!("{}{}", ColorEscape::black(), ColorEscape::reset()),
                ColorEscape::empty(),
            )
        };

        // Write box around top layer
        if self.name.is_none() {
            write!(
                target,
                "{}{}┌{}┐{}",
                cursor::Goto(x_off, y_off),
                c_s,
                delim_hor,
                c_e,
            )
            .unwrap();
        } else {
            write!(
                target,
                "{}{}┌{}┐{}",
                cursor::Goto(x_off, y_off),
                c_s,
                {
                    let name = self.name.unwrap();
                    let delim = "─".repeat(self.top_dimensions.0 as usize - name.len());
                    let mut mid = delim.len() / 2;
                    while delim.get(..mid).is_none() && mid > 0 {
                        mid -= 1;
                    }
                    let (delim_l, delim_r) = delim.split_at(mid);
                    format!("{}{}{}", delim_l, name, delim_r)
                },
                c_e,
            )
            .unwrap();
        }
        for i in 1..=self.top_dimensions.1 {
            write!(
                target,
                "{}{}│{}{}{}│{}",
                cursor::Goto(x_off, y_off + i),
                c_s,
                c_e,
                cursor::Goto(x_off + 1 + self.top_dimensions.0, y_off + i),
                c_s,
                c_e,
            )
            .unwrap();
        }
        write!(
            target,
            "{}{}└{}┘{}",
            cursor::Goto(x_off, y_off + self.top_dimensions.1 + 1),
            c_s,
            delim_hor,
            c_e,
        )
        .unwrap();

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

pub struct OwnedOverlay<T>
where
    T: Component + Send,
{
    top: T,
    below: ManuallyDrop<Box<Component>>,
    top_dimensions: (u16, u16),
    box_color: Option<ColorEscape>,
    name: Option<String>,
}
impl<T> OwnedOverlay<T>
where
    T: Component + Send,
{
    pub fn new<J: Into<Option<String>>>(
        top: T,
        below: Box<Component>,
        top_dimensions: (u16, u16),
        box_color: Option<ColorEscape>,
        name: J,
    ) -> OwnedOverlay<T> {
        assert!(top_dimensions.0 > 0 && top_dimensions.1 > 0);
        OwnedOverlay {
            top,
            below: ManuallyDrop::new(below),
            top_dimensions,
            box_color,
            name: name.into(),
        }
    }
}

impl<T> Component for OwnedOverlay<T> where T: Component + Send {}

impl<T> Renderable for OwnedOverlay<T>
where
    T: Component + Send,
{
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        BorrowedOverlay::new(
            &mut self.top,
            &mut **self.below,
            self.top_dimensions,
            self.box_color.as_ref(),
            self.name.as_ref().map(|s| &s[..]),
        )
        .render(target, width, height, x_off, y_off)
    }
}

impl<T> HandleRpc for OwnedOverlay<T>
where
    T: Component + Send,
{
    fn rpc(&mut self, msg: SMessage) -> bool {
        self.top.rpc(msg.clone());
        self.below.rpc(msg)
    }
}

impl<T> HandleInput for OwnedOverlay<T>
where
    T: Component + Send,
{
    fn input(&mut self, k: Key, w: u16, h: u16) -> InputResult {
        match self.top.input(k, w, h) {
            InputResult::Close => InputResult::ReplaceWith(unsafe {
                Box::from_raw((&mut **self.below) as *mut Component)
            }),
            ret => ret,
        }
    }
}

pub struct Text<T, AX, AY>
where
    T: Borrow<str> + Send,
    AX: x::Align + Send,
    AY: y::Align + Send,
{
    do_goto: bool,
    content: T,
    _align_x: PhantomData<AX>,
    _align_y: PhantomData<AY>,
}

impl<T, AX, AY> Text<T, AX, AY>
where
    T: Borrow<str> + Send,
    AX: x::Align + Send,
    AY: y::Align + Send,
{
    pub fn new(do_goto: bool, t: T) -> Text<T, AX, AY> {
        Text {
            do_goto,
            content: t,
            _align_x: PhantomData,
            _align_y: PhantomData,
        }
    }
}

macro_rules! do_write {
    ($target:expr, $x_off:expr, $y_off:expr, $lit1:expr, $lit2:expr, $ct:expr, $do_goto:expr) => {
        if $do_goto {
            write!($target, $lit1, cursor::Goto($x_off, $y_off), $ct).unwrap();
        } else {
            write!($target, $lit2, $ct).unwrap();
        }
    };
}

// TODO: This needs a major overhaul, it's too ugly and complex, also no good multiline support
// FIXME: If the width is 2. weird stuff happens
impl<T, AX, AY> Renderable for Text<T, AX, AY>
where
    T: Borrow<str> + Send,
    AX: x::Align + Send,
    AY: y::Align + Send,
{
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        if width <= 1 || height == 0 {
            return;
        }

        let content = self.content.borrow();
        let x_off = x_off
            + match AX::align_offset(&[content], width) {
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
            do_write!(target, x_off, y_off, "{}{}", "{}", content, self.do_goto);
        } else {
            let mut chunks = content
                .graphemes(true)
                // Version of .chunks that preserves control codes, unfortunately termion does not
                // provide access to its parsing functions
                .fold(
                    // idx of control, idx of str, current str len, inside_esc, control code, str
                    (0, 0, 0, false, Vec::new(), vec![String::new()]),
                    |mut acc, g| {
                        if g == "\x1B" {
                            // Control code starts

                            acc.3 = true;
                            acc.4.push(String::from("\x1B"));
                            acc.5[acc.1].push('\x1B');
                        } else if acc.3 && g != "m" {
                            // Control code content

                            acc.4[acc.0].push_str(g);
                            acc.5[acc.1].push_str(g);
                        } else if acc.3 && g == "m" {
                            // Control code stops

                            acc.3 = false;
                            match &*acc.4[acc.0] {
                                // Reset styling, or fg color, or bg color
                                "\x1B[m" | "\x1B[39m" | "\x1B49m" => {
                                    let mut r_idx = 0;
                                    for (i, c) in acc.4.iter().skip(1).enumerate() {
                                        if !c.contains(';')
                                            || c.starts_with("38;")
                                            || c.starts_with("48;")
                                        {
                                            r_idx = acc.4.len() - 2 - i;
                                            break;
                                        }
                                    }

                                    let l = acc.4.len();
                                    acc.4.remove(l - 1);
                                    acc.4.remove(r_idx);
                                }
                                _ => {
                                    acc.4[acc.0].push('m');
                                    acc.5[acc.1].push('m');
                                }
                            }
                        } else {
                            // No control code

                            let l = utils::count(g);
                            if acc.2 + l >= width as _ {
                                acc.5.push(String::new());
                                acc.4.reverse();
                                for esc in acc.4.drain(..) {
                                    if esc.starts_with("\x1B[38;") {
                                        acc.5[acc.1].push_str(ColorEscape::reset().inner());
                                    } else if esc.starts_with("\x1B[48;") {
                                        acc.5[acc.1].push_str(ColorEscape::reset_bg().inner());
                                    } else {
                                        acc.5[acc.1].push_str(&format!("{}", style::Reset));
                                    }
                                    acc.5[acc.1 + 1].push_str(&esc);
                                }
                                acc.0 = 0;

                                acc.1 += 1;
                                acc.5[acc.1].push_str(g);
                                acc.2 = l;
                            } else {
                                acc.2 += l;
                                acc.5[acc.1].push_str(g);
                            }
                        }
                        acc
                    },
                )
                .5
                .into_iter()
                .peekable();
            let mut i = 0;
            while let Some(chunk) = chunks.next() {
                if let Some(n_chunk) = chunks.peek() {
                    if utils::count_without_styling(n_chunk) > 1 {
                        if i + 1 >= height {
                            // Truncate
                            do_write!(
                                target,
                                x_off,
                                y_off + i,
                                "{}{}…",
                                "{}…",
                                chunk,
                                self.do_goto
                            );
                            break;
                        } else {
                            // Wrap
                            do_write!(
                                target,
                                x_off,
                                y_off + i,
                                "{}{}-",
                                "{}-",
                                chunk,
                                self.do_goto
                            );
                        }
                        i += 1;
                    } else {
                        // Next chunk is small and the last chunk, needs no new line
                        do_write!(
                            target,
                            x_off,
                            y_off + 1,
                            "{}{}",
                            "{}",
                            format!("{}{}", chunk, n_chunk),
                            self.do_goto
                        );
                        break;
                    }
                } else {
                    // Last chunk
                    do_write!(target, x_off, y_off + 1, "{}{}", "{}", chunk, self.do_goto);
                    break;
                }
            }
        }
    }
}

// FIXME: The grapheme usage is quite allocation heavy
#[derive(Clone)]
pub struct Input {
    content: String,
    pos: usize,
}

impl Input {
    pub fn from<T: Into<Option<usize>>>(content: String, pos: T) -> Input {
        Input {
            pos: pos
                .into()
                .and_then(|pos| {
                    assert!(pos > 0);
                    Some(pos)
                })
                .unwrap_or_else(|| content.graphemes(true).count() + 1),
            content,
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

    pub fn home(&mut self) {
        self.pos = 1;
    }

    pub fn end(&mut self) {
        self.pos = self.content.len() + 1
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
        let offset = self
            .content
            .graphemes(true)
            .take(self.pos - 1)
            .collect::<String>()
            .bytes()
            .count();
        self.content.insert(offset, c);
        self.pos += 1;
    }
    pub fn backspace(&mut self) {
        if self.pos > 1 {
            self.content = self
                .content
                .graphemes(true)
                .take(self.pos - 2)
                .chain(self.content.graphemes(true).skip(self.pos - 2).skip(1))
                .collect();
            self.pos -= 1;
        }
    }
    pub fn delete(&mut self) {
        if !self.content.is_empty() && self.pos <= self.content.graphemes(true).count() {
            self.content = self
                .content
                .graphemes(true)
                .take(self.pos - 1)
                .chain(self.content.graphemes(true).skip(self.pos - 1).skip(1))
                .collect();
        }
    }

    pub fn format_active(&self) -> String {
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
                &self
                    .content
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
                if self.pos < len {
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
    pub fn from<T: Into<Option<usize>>>(content: String, pos: T) -> PasswordInput {
        PasswordInput(Input::from(content, pos))
    }
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
    pub fn format_active(&self) -> String {
        let len = self.content.graphemes(true).count();
        let stars = "*".repeat(len);
        if self.pos > len {
            format!("{}{} {}", &stars, style::Underline, style::NoUnderline,)
        } else {
            format!(
                "{}{}{}{}{}",
                &stars[..self.pos.saturating_sub(1)],
                style::Underline,
                if self.pos > 1 || !self.content.is_empty() {
                    &stars[self.pos.saturating_sub(1)..self.pos]
                } else {
                    " "
                },
                style::NoUnderline,
                if self.pos < len {
                    &stars[self.pos..]
                } else {
                    ""
                }
            )
        }
    }
    pub fn format_inactive(&self) -> String {
        "*".repeat(self.content.graphemes(true).count())
    }
}

pub struct RenderFn<F>
where
    F: Fn(&mut Vec<u8>, u16, u16, u16, u16) + Send,
{
    ct: F,
}
impl<F> RenderFn<F>
where
    F: Fn(&mut Vec<u8>, u16, u16, u16, u16) + Send,
{
    pub fn new(fun: F) -> RenderFn<F> {
        RenderFn { ct: fun }
    }
}
impl<F> Renderable for RenderFn<F>
where
    F: Fn(&mut Vec<u8>, u16, u16, u16, u16) + Send,
{
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        (self.ct)(target, width, height, x_off, y_off);
    }
}

pub struct RenderStateFn<F, T>
where
    F: Fn(&mut Vec<u8>, u16, u16, u16, u16, &mut T) + Send,
    T: Send,
{
    ct: F,
    state: T,
}
impl<F, T> RenderStateFn<F, T>
where
    F: Fn(&mut Vec<u8>, u16, u16, u16, u16, &mut T) + Send,
    T: Send,
{
    pub fn new(fun: F, state: T) -> RenderStateFn<F, T> {
        RenderStateFn { ct: fun, state }
    }
}
impl<F, T> Renderable for RenderStateFn<F, T>
where
    F: Fn(&mut Vec<u8>, u16, u16, u16, u16, &mut T) + Send,
    T: Send,
{
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        (self.ct)(target, width, height, x_off, y_off, &mut self.state);
    }
}

pub struct CloseOnInput<'t, T>
where
    T: Renderable + HandleRpc + Send + Sync,
{
    content: T,
    trigger: &'t [Key],
}

impl<'t, T> CloseOnInput<'t, T>
where
    T: Renderable + HandleRpc + Send + Sync,
{
    pub fn new(content: T, trigger: &'t [Key]) -> CloseOnInput<'t, T> {
        CloseOnInput { content, trigger }
    }
}

impl<'t, T> Component for CloseOnInput<'t, T> where T: Renderable + HandleRpc + Send + Sync {}

impl<'t, T> Renderable for CloseOnInput<'t, T>
where
    T: Renderable + HandleRpc + Send + Sync,
{
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        self.content.render(target, width, height, x_off, y_off);
    }
}

impl<'t, T> HandleInput for CloseOnInput<'t, T>
where
    T: Renderable + HandleRpc + Send + Sync,
{
    fn input(&mut self, k: Key, _: u16, _: u16) -> InputResult {
        if self.trigger.is_empty() || self.trigger.contains(&k) {
            InputResult::Close
        } else {
            InputResult::Rerender
        }
    }
}

impl<'t, T> HandleRpc for CloseOnInput<'t, T>
where
    T: Renderable + HandleRpc + Send + Sync,
{
    fn rpc(&mut self, msg: SMessage) -> bool {
        self.content.rpc(msg)
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
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16) {
        self.content.render(target, width, height, x_off, y_off);
    }
}

impl<T> HandleRpc for IgnoreRpc<T>
where
    T: Renderable,
{
    fn rpc(&mut self, _: SMessage) -> bool {
        false
    }
}
