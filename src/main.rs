// We always want to run Clippy on this code and have it as a dependency. Don't
// do this for real Rust applications.
#![deny(warnings)]
#![feature(plugin)]
#![plugin(clippy)]

extern crate hyper;
extern crate rustnish;


fn main() {
    let port: u16 = 9090;
    // If a function returns something in Rust you can't ignore it, so we need this superflous
    // unused variable here. Starting it with "_" tells the compiler to ignore it.
    let _listening = rustnish::start_server(port);
}
