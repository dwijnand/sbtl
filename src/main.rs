//! A Rust port of sbt-extras.
//! Author: Dale Wijnand <dale.wijnand@gmail.com>

extern crate curl;
extern crate jsonrpc_lite;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate serde_json;

mod launcher;
mod client;

fn main() {
    launcher::Launcher::new().run();
}
