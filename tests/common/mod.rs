extern crate hyper;
extern crate futures;
extern crate rustnish;
extern crate tokio_core;
extern crate error_chain;

use hyper::{Client, Method, Uri};
use hyper::server::{Http, Request, Response, Service};
use std::sync::mpsc;
use std::thread;
use futures::Future;
use tokio_core::reactor::Core;
use std::str;

struct DummyServer;

// A dummy upstream HTTP server for testing that returns the received HTTP
// request in the response body.
impl Service for DummyServer {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = futures::future::FutureResult<Self::Response, Self::Error>;

    fn call(&self, request: Request) -> Self::Future {
        let mut response = Response::new();

        response.set_body(format!("{:?}", request));

        futures::future::ok(response)
    }
}

// Starts a dummy server in a separate thread.
pub fn start_dummy_server(port: u16) -> thread::JoinHandle<()> {
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
pub fn client_get(url: Uri) -> Response {
    let mut core = Core::new().unwrap();
    let client = Client::new(&core.handle());

    let work = client.get(url).and_then(Ok);
    core.run(work).unwrap()
}

pub fn client_post(url: Uri, body: &str) -> Response {
    let mut core = Core::new().unwrap();
    let client = Client::new(&core.handle());

    let mut req = Request::new(Method::Post, url);
    let body_data = String::from(body);
    req.set_body(body_data);

    let work = client.request(req).and_then(Ok);
    core.run(work).unwrap()
}

// Since it so complicated to make a client request with a Tokio core we have
// this helper function.
pub fn client_request(request: Request) -> Response {
    let mut core = Core::new().unwrap();
    let client = Client::new(&core.handle());

    let work = client.request(request).and_then(Ok);
    core.run(work).unwrap()
}
