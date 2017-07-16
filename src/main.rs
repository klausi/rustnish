#![deny(warnings)]

extern crate rustnish;


fn main() {
    let port: u16 = 9090;
    let upstream_port: u16 = 82;
    let thread = rustnish::start_server(port, upstream_port);
    let _guard = thread.join();
}
