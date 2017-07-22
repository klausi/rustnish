#![deny(warnings)]

extern crate rustnish;
extern crate error_chain;


fn main() {
    let port: u16 = 9090;
    let upstream_port: u16 = 80;

    let _guard = match rustnish::start_server_blocking(port, upstream_port) {
        Err(ref e) => {
            use std::io::Write;
            use error_chain::ChainedError; // trait which holds `display`
            let stderr = &mut ::std::io::stderr();
            let errmsg = "Error writing to stderr";

            writeln!(stderr, "{}", e.display()).expect(errmsg);
            ::std::process::exit(1);
        }
        Ok(_) => (),
    };
}
