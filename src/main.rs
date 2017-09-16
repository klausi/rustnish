#![deny(warnings)]

extern crate rustnish;
extern crate error_chain;


fn main() {
    let port: u16 = 9090;
    let upstream_port: u16 = 80;

    if let Err(ref e) = rustnish::start_server_blocking(port, upstream_port) {
        use std::io::Write;
        use error_chain::ChainedError; // trait which holds `display`
        let stderr = &mut ::std::io::stderr();
        let errmsg = "Error writing to stderr";

        writeln!(stderr, "{}", e.display_chain()).expect(errmsg);
        ::std::process::exit(1);
    };
}
