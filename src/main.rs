#![deny(warnings)]

extern crate error_chain;
extern crate rustnish;

fn main() {
    let port: u16 = 9090;
    let upstream_port: u16 = 80;

    rustnish::start_server_blocking(port, upstream_port);
}
