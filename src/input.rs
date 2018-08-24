// Copyright (C) 2017  ParadoxSpiral
//
// This file is part of axon.
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
use termion::input::TermRead;

pub fn start() {
    ::std::thread::spawn(|| {
        let stdin = ::std::io::stdin();
        for ev in stdin.lock().keys() {
            match ev {
                Ok(k) => {
                    ::tui::view::notify_input(k);
                }
                Err(e) => {
                    ::VIEW.overlay("Input".to_owned(), e.to_string(), Some(color::Red));
                }
            };
        }
    });
}
