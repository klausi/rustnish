extern crate hyper;
extern crate futures;
extern crate rustnish;
extern crate tokio_core;

use hyper::Client;
use hyper::Uri;
use hyper::server::{Http, Request, Response, Service};
use std::thread;
use futures::{Future, Stream};
use tokio_core::reactor::Core;
use std::str;

struct DummyServer;

// A dummy upstream HTTP server for testing that just always return hello.
impl Service for DummyServer {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = futures::future::FutureResult<Self::Response, Self::Error>;

    fn call(&self, _request: Request) -> Self::Future {
        let mut response = Response::new();
        response.set_body("hello");
        futures::future::ok(response)
    }
}

// Starts a dummy server in a separate thread.
fn start_dummy_server(port: u16) -> thread::JoinHandle<()> {
    let thread = thread::Builder::new()
        .name("test-server".to_owned())
        .spawn(move || {
                   let address = "127.0.0.1:".to_owned() + &port.to_string();
                   let addr = address.parse().unwrap();

                   let server = Http::new().bind(&addr, || Ok(DummyServer)).unwrap();
                   server.run().unwrap();
               })
        .unwrap();

    thread
}

// Since it so complicated to make a client request with a Tokio core we have
// this helper function.
fn client_get(url: Uri) -> Response {
    let mut core = Core::new().unwrap();
    let client = Client::new(&core.handle());

    let work = client.get(url).and_then(|response| Ok(response));
    core.run(work).unwrap()
}

#[test]
fn test_pass_through() {
    let port = 9090;
    let upstream_port = 9091;

    // Start a dummy server on port 9091 that just returns a hello.
    let _dummy_server = start_dummy_server(upstream_port);

    // Start our reverse proxy which forwards to the dummy server.
    let _proxy = rustnish::start_server(port, upstream_port);

    // Make a request to the proxy and check if we get the hello back.
    let url = ("http://127.0.0.1:".to_string() + &port.to_string())
        .parse()
        .unwrap();
    let response = client_get(url);

    assert_eq!(Ok("hello"),
               str::from_utf8(&response.body().concat2().wait().unwrap()));
}
