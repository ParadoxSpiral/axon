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

use termion::color;

use std::fmt;

// TODO: This should be &'static str, but termion's design currently doesn't allow that-
// With more const fn stuff, this could possibly be done with &*format!(...)
/// This codes termion's Color so as to avoid passing around Box<dyn Color>
pub struct ColorEscape(String);

impl ColorEscape {
    pub fn empty() -> ColorEscape {
        ColorEscape(String::new())
    }
    pub fn inner(&self) -> &String {
        &self.0
    }

    pub fn reset() -> ColorEscape {
        ColorEscape(format!("{}", color::Fg(color::Reset)))
    }
    pub fn black() -> ColorEscape {
        ColorEscape(format!("{}", color::Fg(color::Black)))
    }
    pub fn red() -> ColorEscape {
        ColorEscape(format!("{}", color::Fg(color::Red)))
    }
    pub fn cyan() -> ColorEscape {
        ColorEscape(format!("{}", color::Fg(color::Cyan)))
    }

    pub fn reset_bg() -> ColorEscape {
        ColorEscape(format!("{}", color::Bg(color::Reset)))
    }
    pub fn red_bg() -> ColorEscape {
        ColorEscape(format!("{}", color::Bg(color::Red)))
    }
}

impl fmt::Display for ColorEscape {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(fmt, "{}", self.0)
    }
}
