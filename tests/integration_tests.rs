extern crate hyper;
extern crate futures;
extern crate rustnish;
extern crate tokio_core;

use hyper::{Client, Method, StatusCode, Uri};
use hyper::header::Host;
use hyper::server::{Http, Request, Response, Service};
use std::sync::mpsc;
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
    // We need to block until the server has bound successfully to the port, so
    // we block on this channel before we return. As soon as the thread sends
    // out the signal we can return.
    let (addr_tx, addr_rx) = mpsc::channel();

    let thread = thread::Builder::new()
        .name("test-server".to_owned())
        .spawn(move || {
            let address = "127.0.0.1:".to_owned() + &port.to_string();
            let addr = address.parse().unwrap();

            let server = Http::new().bind(&addr, || Ok(DummyServer)).unwrap();
            addr_tx.send(true).unwrap();
            server.run().unwrap();
        })
        .unwrap();

    let _bind_ready = addr_rx.recv().unwrap();

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

// Since it so complicated to make a client request with a Tokio core we have
// this helper function.
fn client_request(request: Request) -> Response {
    let mut core = Core::new().unwrap();
    let client = Client::new(&core.handle());

    let work = client.request(request).and_then(|response| Ok(response));
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

    assert_eq!(
        Ok("hello"),
        str::from_utf8(&response.body().concat2().wait().unwrap())
    );
}

// Tests that if the proxy cannot connect to upstream it returns a 502 response.
#[test]
fn test_upstream_down() {
    let port = 9092;
    let upstream_port = 9093;

    let _proxy = rustnish::start_server(port, upstream_port);

    // Make a request to the proxy and check the response.
    let url = ("http://127.0.0.1:".to_string() + &port.to_string())
        .parse()
        .unwrap();
    let response = client_get(url);

    assert_eq!(StatusCode::BadGateway, response.status());
    assert_eq!(
        Ok("Something went wrong, please try again later."),
        str::from_utf8(&response.body().concat2().wait().unwrap())
    );
}

// Tests that an invalid HTTP host header does not cause a panic.
#[test]
fn test_invalid_host() {
    let port = 9094;
    let upstream_port = 9095;

    let _proxy = rustnish::start_server(port, upstream_port);

    let url = ("http://127.0.0.1:".to_string() + &port.to_string())
        .parse()
        .unwrap();
    let mut request = Request::new(Method::Get, url);
    request.headers_mut().set(Host::new("$$$", None));

    let response = client_request(request);

    // The proxy just tries to forward that as is, but no one is listening.
    assert_eq!(StatusCode::BadGateway, response.status());
    assert_eq!(
        Ok("Something went wrong, please try again later."),
        str::from_utf8(&response.body().concat2().wait().unwrap())
    );
}
