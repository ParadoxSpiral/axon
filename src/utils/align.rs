// Copyright (C) 2017  ParadoxSpiral
//
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

pub mod x {
    use super::super::*;
    use std::io::Write;
    use termion::cursor;

    pub trait Align {
        fn align_offset(lines: &[&str], width: u16) -> Alignment;
        fn align(_target: &mut Vec<u8>, _lines: &[&str]) {
            unimplemented!()
        }
    }

    pub enum Alignment {
        Single(u16),
        Each(Vec<u16>),
    }

    pub struct Left {}
    impl Align for Left {
        fn align(target: &mut Vec<u8>, lines: &[&str]) {
            for l in lines {
                write!(
                    target,
                    "{}{}{}",
                    l,
                    cursor::Left(count_without_styling(l) as u16),
                    cursor::Down(1)
                ).unwrap();
            }
        }
        fn align_offset(_: &[&str], _: u16) -> Alignment {
            Alignment::Single(0)
        }
    }

    pub struct Right {}
    impl Align for Right {
        fn align_offset(lines: &[&str], width: u16) -> Alignment {
            assert!(lines.len() == 1);
            Alignment::Single(width.saturating_sub(count_without_styling(lines[0])))
        }
    }

    pub struct Center {}
    impl Align for Center {
        fn align_offset(lines: &[&str], width: u16) -> Alignment {
            let mut algns = Vec::with_capacity(lines.len());
            for l in lines {
                algns.push((width / 2).saturating_sub((count_without_styling(l) / 2) as u16));
            }
            Alignment::Each(algns)
        }
    }

    pub struct CenterLongestLeft {}
    impl Align for CenterLongestLeft {
        fn align_offset(lines: &[&str], width: u16) -> Alignment {
            let max_len = lines
                .iter()
                .map(|l| count_without_styling(*l))
                .max()
                .unwrap() as u16;

            Alignment::Single((width / 2).saturating_sub(max_len / 2))
        }
    }
}

pub mod y {
    pub trait Align {
        fn align_offset(lines: &[&str], height: u16) -> u16;
    }

    pub struct Top {}
    impl Align for Top {
        fn align_offset(_: &[&str], _: u16) -> u16 {
            0
        }
    }

    pub struct Center {}
    impl Align for Center {
        fn align_offset(lines: &[&str], height: u16) -> u16 {
            (height / 2).saturating_sub(lines.len() as u16 / 2)
        }
    }
}
