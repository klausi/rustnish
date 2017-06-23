extern crate hyper;
extern crate futures;
extern crate tokio_core;

use hyper::Client;
use hyper::server::{Http, Request, Response, Service};
use tokio_core::reactor::Core;
use hyper::StatusCode;

struct Proxy {
    upstream_port: u16,
}

impl Service for Proxy {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = futures::future::FutureResult<Self::Response, Self::Error>;

    fn call(&self, request: Request) -> Self::Future {
        let response = pipe_through(request, self.upstream_port);
        futures::future::ok(response)
    }
}

pub fn start_server(port: u16, upstream_port: u16) {
    let address = "127.0.0.1:".to_owned() + &port.to_string();
    println!("Listening on {}", address);
    let addr = address.parse().unwrap();
    let server = Http::new()
        .bind(&addr, move || Ok(Proxy { upstream_port }))
        .unwrap();
    server.run().unwrap();
}

fn pipe_through(request: Request, upstream_port: u16) -> Response {
    let mut core = Core::new().unwrap();
    let client = Client::new(&core.handle());

    let uri = "http://drupal-8.localhost".parse().unwrap();

    let work = client.get(uri);

    match core.run(work) {
        // Directly pass through the client response.
        Ok(response) => response,
        Err(_) => {
            let mut response = Response::new();
            response.set_status(StatusCode::InternalServerError);
            response.set_body("error");

            response
        }
    }
}
