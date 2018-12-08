// Copy of the example in Hyper except that we use port 9091.

#![deny(warnings)]

use hyper::rt::{self, Future};
use hyper::service::service_fn_ok;
use hyper::{Body, Response, Server};

static PHRASE: &'static [u8] = b"Hello World!";

fn main() {
    let addr = ([127, 0, 0, 1], 9091).into();

    // new_service is run for each connection, creating a 'service'
    // to handle requests for that specific connection.
    let new_service = || {
        // This is the `Service` that will handle the connection.
        // `service_fn_ok` is a helper to convert a function that
        // returns a Response into a `Service`.
        service_fn_ok(|_| Response::new(Body::from(PHRASE)))
    };

    let server = Server::bind(&addr)
        .serve(new_service)
        .map_err(|e| eprintln!("server error: {}", e));

    println!("Listening on http://{}", addr);

    rt::run(server);
}
