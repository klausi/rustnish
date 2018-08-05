#![deny(warnings)]

extern crate error_chain;
extern crate rustnish;

fn main() {
    let port: u16 = 9090;
    let upstream_port: u16 = 80;

    if let Err(ref e) = rustnish::start_server_blocking(port, upstream_port) {
        use error_chain::ChainedError;
        use std::io::Write; // trait which holds `display`
        let stderr = &mut ::std::io::stderr();

        writeln!(stderr, "{}", e.display_chain()).expect("Error writing to stderr");
        ::std::process::exit(1);
    };
}
