#![feature(test)]

// Performs various timing tests on Rustnish and Varnish.
// Execute with `cargo bench`
// Varnish must be running and configured to listen on port 6081. The backend
// port must be set to 9091.
// Example Varnish configuration in /etc/varnish/default.vcl:
// ```
// vcl 4.0;
// # Default backend definition. Set this to point to your content server.
// backend default {
//    .host = "127.0.0.1";
//    .port = "9091";
// }
//
// sub vcl_recv {
//    return (pass);
// }
// ```

extern crate futures;
extern crate hyper;
extern crate rustnish;
extern crate test;
extern crate tokio_core;

use futures::{future, Future, Stream};
use futures::future::{join_all, loop_fn, Loop};
use tokio_core::reactor::{Core, Handle};
use tokio_core::net::TcpListener;

use hyper::header::{ContentLength, ContentType};
use hyper::server::{self, Service};

#[bench]
fn a_1_request(b: &mut test::Bencher) {
    rustnish::start_server_background(9090, 9091).unwrap();
    bench_requests(b, 1, 1, 9090);
}

#[bench]
fn a_1_request_varnish(b: &mut test::Bencher) {
    // Assume Varnish is already running.
    bench_requests(b, 1, 1, 6081);
}

#[bench]
fn b_10_requests(b: &mut test::Bencher) {
    rustnish::start_server_background(9090, 9091).unwrap();
    bench_requests(b, 10, 1, 9090);
}

#[bench]
fn b_10_requests_varnish(b: &mut test::Bencher) {
    // Assume Varnish is already running.
    bench_requests(b, 10, 1, 6081);
}

#[bench]
fn c_100_requests(b: &mut test::Bencher) {
    rustnish::start_server_background(9090, 9091).unwrap();
    bench_requests(b, 100, 1, 9090);
}

#[bench]
fn c_100_requests_varnish(b: &mut test::Bencher) {
    // Assume Varnish is already running.
    bench_requests(b, 100, 1, 6081);
}

#[bench]
fn d_10_parallel_requests(b: &mut test::Bencher) {
    rustnish::start_server_background(9090, 9091).unwrap();
    bench_requests(b, 10, 10, 9090);
}

#[bench]
fn d_10_parallel_requests_varnish(b: &mut test::Bencher) {
    // Assume Varnish is already running.
    bench_requests(b, 10, 10, 6081);
}

#[bench]
fn e_100_parallel_requests(b: &mut test::Bencher) {
    bench_requests(b, 100, 10, 9090);
}

#[bench]
fn e_100_parallel_requests_varnish(b: &mut test::Bencher) {
    // Assume Varnish is already running.
    bench_requests(b, 100, 10, 6081);
}

#[bench]
fn f_1_000_parallel_requests(b: &mut test::Bencher) {
    bench_requests(b, 1_000, 100, 9090);
}

#[bench]
fn f_1_000_parallel_requests_varnish(b: &mut test::Bencher) {
    // Assume Varnish is already running.
    bench_requests(b, 1_000, 100, 6081);
}

fn bench_requests(b: &mut test::Bencher, amount: u32, concurrency: u32, proxy_port: u16) {
    let mut core = Core::new().unwrap();
    let handle = core.handle();
    spawn_hello(&handle);

    let client = hyper::Client::new(&handle);

    let url: hyper::Uri = format!("http://127.0.0.1:{}/get", proxy_port)
        .parse()
        .unwrap();

    b.iter(move || {
        let mut parallel = Vec::new();
        for _i in 0..concurrency {
            let requests_til_done = loop_fn(0, |counter| {
                client
                    .get(url.clone())
                    .and_then(|res| {
                        assert_eq!(
                            res.status(),
                            hyper::StatusCode::Ok,
                            "Varnish did not return a 200 HTTP status code. Make sure Varnish is configured on port {} and the backend port is set to 9091 in /etc/varnish/default.vcl",
                            proxy_port
                        );
                        // Read response body until the end.
                        res.body().for_each(|_chunk| Ok(()))
                    })
                    .and_then(move |_| -> Result<_, hyper::Error> {
                        if counter < (amount / concurrency) {
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
