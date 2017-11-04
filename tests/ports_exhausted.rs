extern crate futures;
extern crate hyper;
extern crate procinfo;
extern crate rustnish;
extern crate tokio_core;

use std::net::ToSocketAddrs;
use futures::Stream;
use futures::future::{join_all, loop_fn, Future, Loop};
use tokio_core::net::TcpStream;
use tokio_core::reactor::Core;

mod common;

// Test that opening large numbers of TCP connections do not crash the server.
#[test]
fn test_ports_exhausted() {
    let port = common::get_free_port();
    let upstream_port = common::get_free_port();

    let _dummy_server = common::start_dummy_server(upstream_port, |r| r);
    let _proxy = rustnish::start_server_background(port, upstream_port);

    let mut core = Core::new().unwrap();
    let handle = core.handle();

    let addr_string = format!("localhost:{}", port);
    let addr = addr_string.to_socket_addrs().unwrap().next().unwrap();

    // Send 100k requests (TCP connections).
    let nr_requests = 100_000;
    let concurrency = 10_000;

    let mut parallel = Vec::new();
    for _i in 0..concurrency {
        let requests_til_done = loop_fn(0, |counter| {
            // Just establish the TCP connection, do nothing otherwise.
            let socket = TcpStream::connect(&addr, &handle);

            socket.then(move |_| -> Result<_, std::io::Error> {
                if counter < (nr_requests / concurrency) {
                    Ok(Loop::Continue(counter + 1))
                } else {
                    Ok(Loop::Break(counter))
                }
            })
        });
        parallel.push(requests_til_done);
    }

    let work = join_all(parallel);
    core.run(work).unwrap();

    // After all those requests our server shoudl still be alive and well.
    let url = ("http://127.0.0.1:".to_string() + &port.to_string())
        .parse()
        .unwrap();
    let response = common::client_get(url);
    let body = response.body().concat2().wait().unwrap();
    let result = std::str::from_utf8(&body).unwrap();

    assert_eq!(
        "Request { method: Get, uri: \"/\", version: Http11, remote_addr:",
        &result[..62]
    );
}
