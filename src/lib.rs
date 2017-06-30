extern crate hyper;
extern crate futures;
extern crate tokio_core;

use hyper::Client;
use hyper::server::{Http, Request, Response, Service};
use tokio_core::net::TcpListener;
use tokio_core::reactor::Core;
use futures::Stream;
use hyper::client::HttpConnector;
use hyper::client::FutureResponse;

struct Proxy {
    upstream_port: u16,
    client: Client<HttpConnector>,
}

impl Service for Proxy {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = FutureResponse;

    fn call(&self, request: Request) -> Self::Future {
        let uri = "http://drupal-8.localhost".parse().unwrap();

        self.client.get(uri)
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
