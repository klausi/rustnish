extern crate futures;
extern crate hyper;

use hyper::{Client, Server, Uri};
use hyper::{Body, Request, Response};
use std::sync::mpsc;
use std::sync::atomic::{AtomicUsize, Ordering, ATOMIC_USIZE_INIT};
use std::thread;
use futures::Future;
use tokio_core::reactor::Core;
use std::str;
use hyper::service::service_fn_ok;

// Return the received request in the response body for testing purposes.
pub fn echo_request(request: Request<Body>) -> Response<Body> {
    Response::builder()
            .body(Body::from(format!("{:?}", request)))
            .unwrap()
}

// Starts a dummy server in a separate thread.
pub fn start_dummy_server(
    port: u16,
    response_function: fn(Request<Body>) -> Response<Body>,
) -> thread::JoinHandle<()> {
    // We need to block until the server has bound successfully to the port, so
    // we block on this channel before we return. As soon as the thread sends
    // out the signal we can return.
    let (addr_tx, addr_rx) = mpsc::channel();

    let thread = thread::Builder::new()
        .name("test-server".to_owned())
        .spawn(move || {
            let address = "127.0.0.1:".to_owned() + &port.to_string();
            let addr = address.parse().unwrap();

            let new_svc = move || {
                service_fn_ok(response_function)
            };

            let server = Server::bind(&addr)
                .serve(new_svc)
                .map_err(|_| ());

            addr_tx.send(true).unwrap();

            // Run this server for... forever!
            hyper::rt::run(server);
        })
        .unwrap();

    let _bind_ready = addr_rx.recv().unwrap();

    thread
}

// Since it so complicated to make a client request with a Hyper runtime we have
// this helper function.
#[allow(dead_code)]
pub fn client_get(url: Uri) -> Response<Body> {
    let mut core = Core::new().unwrap();
    let client = Client::new();

    let work = client.get(url).and_then(Ok);
    core.run(work).unwrap()
}

#[allow(dead_code)]
pub fn client_post(url: Uri, body: &'static str) -> Response<Body> {
    let mut core = Core::new().unwrap();
    let client = Client::new();

    let req = Request::builder()
        .method("POST")
        .uri(url)
        .body(Body::from(body))
        .unwrap();

    let work = client.request(req).and_then(Ok);
    core.run(work).unwrap()
}

// Since it so complicated to make a client request with a Tokio core we have
// this helper function.
#[allow(dead_code)]
pub fn client_request(request: Request<Body>) -> Response<Body> {
    let mut core = Core::new().unwrap();
    let client = Client::new();

    let work = client.request(request).and_then(Ok);
    core.run(work).unwrap()
}

// Returns a local port number that has not been used yet in parallel test
// threads.
pub fn get_free_port() -> u16 {
    static PORT_NR: AtomicUsize = ATOMIC_USIZE_INIT;

    PORT_NR.fetch_add(1, Ordering::SeqCst) as u16 + 9090
}
