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

use lazy_static::lazy_static;
use serde::Deserialize;
use shellexpand;
use toml;

use std::{fs::File, io::Read, path::Path};

lazy_static! {
    pub static ref CONFIG: Config = {
        let path = shellexpand::full("$XDG_CONFIG_HOME/axon.toml")
            .unwrap_or_else(|_| shellexpand::tilde("~/.config/axon.toml"));
        let path = Path::new(&*path);
        if Path::exists(&path) {
            let mut toml = String::new();
            let mut file = File::open(path).unwrap();
            file.read_to_string(&mut toml).unwrap();

            let cfg = toml::from_str::<Config>(&*toml).unwrap();
            if cfg.autoconnect && !cfg.server.is_some() {
                panic!("Set to autoconnect but no server specified");
            }

            cfg
        } else {
            Config::default()
        }
    };
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub server: Option<String>,
    pub pass: Option<String>,
    pub autoconnect: bool,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            server: None,
            pass: None,
            autoconnect: false,
        }
    }
}
