//! A Rust port of sbt-extras.
//! Author: Dale Wijnand <dale.wijnand@gmail.com>

extern crate curl;
extern crate jsonrpc_lite;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate serde_json;

use std::fs::File;

mod launcher;
mod client;

fn main() {
    match File::open("project/target/active.json") {
        Ok(port_file) => client::talk_to_client(port_file),
        Err(_)        => launcher::Launcher::new().run(),
    }
}
