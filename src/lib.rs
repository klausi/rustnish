extern crate hyper;
extern crate futures;
extern crate tokio_core;

use hyper::Client;
use hyper::server::{Http, Request, Response, Service};
use tokio_core::net::TcpListener;
use tokio_core::reactor::Core;
use futures::Stream;
use futures::future::{Either, FutureResult};
use hyper::client::HttpConnector;
use hyper::client::FutureResponse;
use hyper::header::Host;
use hyper::StatusCode;
use hyper::Uri;

struct Proxy {
    upstream_port: u16,
    client: Client<HttpConnector>,
}

impl Service for Proxy {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = Either<FutureResult<Self::Response, Self::Error>, FutureResponse>;

    fn call(&self, request: Request) -> Self::Future {
        let host = match request.headers().get::<Host>() {
            None => {
                return Either::A(futures::future::ok(Response::new()
                                                         .with_status(StatusCode::BadRequest)
                                                         .with_body("No host header in request")));

            }
            Some(h) => h,
        };
        let request_uri = request.uri();
        let upstream_uri = ("http://".to_string() + &host.to_string() + request_uri.path())
            .parse()
            .unwrap();

        Either::B(self.client.get(upstream_uri))
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
    let client = Client::new(&handle);

    let server = listener
        .incoming()
        .for_each(move |(sock, addr)| {
            http.bind_connection(&handle,
                                 sock,
                                 addr,
                                 Proxy {
                                     upstream_port: upstream_port,
                                     client: client.clone(),
                                 });
            Ok(())
        });

    core.run(server).unwrap();
}
