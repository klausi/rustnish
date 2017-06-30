extern crate hyper;
extern crate futures;
extern crate tokio_core;

use hyper::Client;
use hyper::server::{Http, Request, Response, Service};
use tokio_core::reactor::Core;
use hyper::StatusCode;
use tokio_core::net::TcpListener;
use futures::Stream;

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

    // Prepare a Tokio core that we will use for our server and our client.
    let mut core = Core::new().unwrap();
    let handle = core.handle();
    let http = Http::new();
    let listener = TcpListener::bind(&addr, &handle).unwrap();

    let server = listener
        .incoming()
        .for_each(move |(sock, addr)| {
                      http.bind_connection(&handle, sock, addr, Proxy { upstream_port });
                      Ok(())
                  });

    core.run(server).unwrap();
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
