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

/// This codes (erases) termion's Color so as to avoid passing around Box<dyn Color>
pub struct ColorEscape(&'static str);

impl ColorEscape {
    pub fn empty() -> ColorEscape {
        ColorEscape("")
    }
    pub fn inner(&self) -> &'static str {
        self.0
    }

    pub fn reset() -> ColorEscape {
        ColorEscape(color::Reset.fg_str())
    }
    pub fn black() -> ColorEscape {
        ColorEscape(color::Black.fg_str())
    }
    pub fn red() -> ColorEscape {
        ColorEscape(color::Red.fg_str())
    }
    pub fn cyan() -> ColorEscape {
        ColorEscape(color::Cyan.fg_str())
    }

    pub fn reset_bg() -> ColorEscape {
        ColorEscape(color::Reset.bg_str())
    }
    pub fn red_bg() -> ColorEscape {
        ColorEscape(color::Red.bg_str())
    }
}

impl fmt::Display for ColorEscape {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(fmt, "{}", self.0)
    }
}
