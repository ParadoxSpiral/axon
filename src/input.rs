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

use bytes::BytesMut;
use termion::event::{self, Event, Key};
use tokio::{codec, io, prelude::*};

use std::io::Error;

use crate::{tui::view::Notify, utils::color::ColorEscape};

pub fn start() -> impl Future<Item = (), Error = ()> {
    codec::FramedRead::new(io::stdin(), InputCodec)
        .map_err(|e| {
            Notify::overlay("Input".to_owned(), e.to_string(), Some(ColorEscape::red()));
        })
        .for_each(|key| {
            Notify::input(key);
            Ok(())
        })
}

struct InputCodec;
impl codec::Decoder for InputCodec {
    type Item = Key;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Key>, Error> {
        if src.len() == 0 {
            return Ok(None);
        }

        // Since parse_event does not return how many bytes were read, we count that ourself
        let mut count = 1;
        if let Ok(ev) = event::parse_event(
            src[0],
            &mut src.iter().skip(1).inspect(|_| count += 1).map(|b| Ok(*b)),
        ) {
            src.advance(count);
            match ev {
                Event::Mouse(_) | Event::Unsupported(_) => Ok(None),
                Event::Key(k) => Ok(Some(k)),
            }
        } else {
            Ok(None)
        }
    }
}
