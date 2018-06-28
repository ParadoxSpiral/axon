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

pub mod panels;
mod view;
pub mod widgets;

pub use self::view::View;

use synapse_rpc::message::SMessage;
use termion::event::Key;

use rpc::RpcContext;

pub trait Component: Renderable + HandleInput + HandleRpc {}

pub trait Renderable {
    fn name(&self) -> String {
        "unnamed".to_owned()
    }
    fn render(&mut self, target: &mut Vec<u8>, width: u16, height: u16, x_off: u16, y_off: u16);
}

pub trait HandleInput {
    fn input(&mut self, ctx: &RpcContext, k: Key, width: u16, height: u16) -> InputResult;
}

pub trait HandleRpc {
    fn rpc(&mut self, ctx: &RpcContext, msg: SMessage) -> bool;
}

pub enum InputResult {
    Close,
    Rerender,
    ReplaceWith(Box<Component>),
    // A key was not used by any component below the current one
    Key(Key),
}
