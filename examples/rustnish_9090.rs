// Copy of main.rs that uses upstream port 9091 instead of 80.
#![deny(warnings)]

extern crate error_chain;
extern crate rustnish;

fn main() {
    let port: u16 = 9090;
    let upstream_port: u16 = 9091;

    if let Err(ref e) = rustnish::start_server_blocking(port, upstream_port) {
        use error_chain::ChainedError;
        use std::io::Write; // trait which holds `display`
        let stderr = &mut ::std::io::stderr();
        let errmsg = "Error writing to stderr";

        writeln!(stderr, "{}", e.display_chain()).expect(errmsg);
        ::std::process::exit(1);
    };
}
