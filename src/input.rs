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
use log::debug;
use termion::event::{self, Event, Key};
use tokio::{codec, io, prelude::*};

use std::io::Error;

// FIXME: Due to `https://github.com/tokio-rs/tokio/issues/589` we currently need to handle stdin
// in its own thread
pub fn stream() -> impl Stream<Item = Key, Error = (String, String)> {
    //codec::FramedRead::new(io::stdin(), InputCodec)
    //    .inspect(|key| debug!("Decoded: {:?}", key))
    //    .map_err(|e| ("Input".to_owned(), e.to_string()))

    let (mut s, r) = futures::sync::mpsc::channel(5);
    std::thread::spawn(move || {
        use termion::input::TermRead;
        for k in std::io::stdin().keys() {
            debug!("Decoded: {:?}", k);
            s.try_send(k.unwrap()).unwrap();
        }
    });

    r.map_err(|_| unreachable!())
}

struct InputCodec;
impl codec::Decoder for InputCodec {
    type Item = Key;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Key>, Error> {
        if src.is_empty() {
            return Ok(None);
        }

        // A \x1B is either Key::Esc or starts an escape sequence, parse_event does not
        // parse Key::Esc because termion's design is a bit weird
        if src[0] == b'\x1B' && src.len() == 1
            || src.len() >= 2 && src[0] == b'\x1B' && src[1] == b'\x1B'
        {
            src.advance(1);
            return Ok(Some(Key::Esc));
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
            // This is either an invalid/unsupported escape, or not enough bytes were available at
            // the time this method was called, which seems nigh impossible since bytes written
            // to stdin in one flush will be given to us in one go
            src.clear();
            Ok(None)
        }
    }
}
