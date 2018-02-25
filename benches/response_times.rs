#![feature(test)]

extern crate futures;
extern crate hyper;
extern crate rustnish;
extern crate test;
extern crate tokio_core;

use futures::{future, Future, Stream};
use tokio_core::reactor::{Core, Handle};
use tokio_core::net::TcpListener;

use hyper::header::{ContentLength, ContentType};
use hyper::server::{self, Service};

#[bench]
fn one_request(b: &mut test::Bencher) {
    let mut core = Core::new().unwrap();
    let handle = core.handle();
    spawn_hello(&handle);

    rustnish::start_server_background(9090, 9091).unwrap();

    let client = hyper::Client::new(&handle);

    let url: hyper::Uri = "http://127.0.0.1:9090/get".parse().unwrap();

    b.iter(move || {
        let work = client.get(url.clone()).and_then(|res| {
            assert_eq!(
                res.status(),
                hyper::StatusCode::Ok,
                "Rustnish did not return a 200 HTTP status code"
            );
            // Read response body until the end.
            res.body().for_each(|_chunk| Ok(()))
        });

        core.run(work).unwrap();
    });
}

#[bench]
fn one_request_varnish(b: &mut test::Bencher) {
    let mut core = Core::new().unwrap();
    let handle = core.handle();
    spawn_hello(&handle);

    let client = hyper::Client::new(&handle);

    let url: hyper::Uri = "http://127.0.0.1:6081/get".parse().unwrap();

    b.iter(move || {
        let work = client.get(url.clone()).and_then(|res| {
            assert_eq!(
                res.status(),
                hyper::StatusCode::Ok,
                "Varnish did not return a 200 HTTP status code. Make sure Varnish is configured on port 6081 and the backend port is set to 9091 in /etc/varnish/default.vcl"
            );
            // Read response body until the end.
            res.body().for_each(|_chunk| Ok(()))
        });

        core.run(work).unwrap();
    });
}

static PHRASE: &'static [u8] = b"Hello, World!";

#[derive(Clone, Copy)]
struct Hello;

impl Service for Hello {
    type Request = server::Request;
    type Response = server::Response;
    type Error = hyper::Error;
    type Future = future::FutureResult<Self::Response, hyper::Error>;
    fn call(&self, _req: Self::Request) -> Self::Future {
        future::ok(
            server::Response::new()
                .with_header(ContentLength(PHRASE.len() as u64))
                .with_header(ContentType::plaintext())
                .with_body(PHRASE),
        )
    }
}

// Start a simple hello server on port 9091. This will be our upstream backend
// for testing.
fn spawn_hello(handle: &Handle) {
    let addr = "127.0.0.1:9091".parse().unwrap();
    let listener = TcpListener::bind(&addr, handle).unwrap();

    let handle2 = handle.clone();
    let http = hyper::server::Http::<hyper::Chunk>::new();
    handle.spawn(
        listener
            .incoming()
            .for_each(move |(socket, _addr)| {
                handle2.spawn(
                    http.serve_connection(socket, Hello)
                        .map(|_| ())
                        .map_err(|_| ()),
                );
                Ok(())
            })
            .then(|_| Ok(())),
    );
}
