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
extern crate tokio;
extern crate tokio_core;

use futures::future::{join_all, loop_fn, Loop};
use futures::{Future, Stream};
use hyper::service::service_fn_ok;
use hyper::Server;
use tokio::runtime::Runtime;
use tokio_core::reactor::Core;

use hyper::StatusCode;
use hyper::{Body, Response};

#[bench]
fn a_1_request(b: &mut test::Bencher) {
    let runtime = rustnish::start_server_background(9090, 9091).unwrap();
    bench_requests(b, 1, 1, 9090, runtime);
}

#[bench]
fn a_1_request_varnish(b: &mut test::Bencher) {
    // Assume Varnish is already running.
    let runtime = Runtime::new().unwrap();
    bench_requests(b, 1, 1, 6081, runtime);
}

#[bench]
fn b_10_requests(b: &mut test::Bencher) {
    let runtime = rustnish::start_server_background(9090, 9091).unwrap();
    bench_requests(b, 10, 1, 9090, runtime);
}

#[bench]
fn b_10_requests_varnish(b: &mut test::Bencher) {
    // Assume Varnish is already running.
    let runtime = Runtime::new().unwrap();
    bench_requests(b, 10, 1, 6081, runtime);
}

#[bench]
fn c_100_requests(b: &mut test::Bencher) {
    let runtime = rustnish::start_server_background(9090, 9091).unwrap();
    bench_requests(b, 100, 1, 9090, runtime);
}

#[bench]
fn c_100_requests_varnish(b: &mut test::Bencher) {
    // Assume Varnish is already running.
    let runtime = Runtime::new().unwrap();
    bench_requests(b, 100, 1, 6081, runtime);
}

#[bench]
fn d_10_parallel_requests(b: &mut test::Bencher) {
    let runtime = rustnish::start_server_background(9090, 9091).unwrap();
    bench_requests(b, 10, 10, 9090, runtime);
}

#[bench]
fn d_10_parallel_requests_varnish(b: &mut test::Bencher) {
    // Assume Varnish is already running.
    let runtime = Runtime::new().unwrap();
    bench_requests(b, 10, 10, 6081, runtime);
}

#[bench]
fn e_100_parallel_requests(b: &mut test::Bencher) {
    let runtime = rustnish::start_server_background(9090, 9091).unwrap();
    bench_requests(b, 100, 10, 9090, runtime);
}

#[bench]
fn e_100_parallel_requests_varnish(b: &mut test::Bencher) {
    // Assume Varnish is already running.
    let runtime = Runtime::new().unwrap();
    bench_requests(b, 100, 10, 6081, runtime);
}

#[bench]
fn f_1_000_parallel_requests(b: &mut test::Bencher) {
    let runtime = rustnish::start_server_background(9090, 9091).unwrap();
    bench_requests(b, 1_000, 100, 9090, runtime);
}

#[bench]
fn f_1_000_parallel_requests_varnish(b: &mut test::Bencher) {
    // Assume Varnish is already running.
    let runtime = Runtime::new().unwrap();
    bench_requests(b, 1_000, 100, 6081, runtime);
}

fn bench_requests(
    b: &mut test::Bencher,
    amount: u16,
    concurrency: u16,
    proxy_port: u16,
    runtime: Runtime,
) {
    let mut core = Core::new().unwrap();
    let mut rt = Runtime::new().unwrap();
    spawn_hello(&mut rt);

    let client = hyper::Client::new();

    let url: hyper::Uri = format!("http://127.0.0.1:{}/get", proxy_port)
        .parse()
        .unwrap();

    b.iter(move || {
        let mut parallel = Vec::with_capacity(concurrency as usize);
        for _i in 0..concurrency {
            let requests_til_done = loop_fn(0, |counter| {
                client
                    .get(url.clone())
                    .and_then(|res| {
                        assert_eq!(
                            res.status(),
                            StatusCode::OK,
                            "Varnish did not return a 200 HTTP status code. Make sure Varnish is configured on port {} and the backend port is set to 9091 in /etc/varnish/default.vcl",
                            proxy_port
                        );
                        // Read response body until the end.
                        res.into_body().for_each(|_chunk| Ok(()))
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
    rt.shutdown_now().wait().unwrap();
    runtime.shutdown_now().wait().unwrap();
}

static TEXT: &str = "Hello, World!";

fn spawn_hello(rt: &mut Runtime) {
    let addr = ([127, 0, 0, 1], 9091).into();

    let new_svc = || service_fn_ok(|_req| Response::new(Body::from(TEXT)));

    let server = Server::bind(&addr)
        .serve(new_svc)
        .map_err(|e| eprintln!("server error: {}", e));

    rt.spawn(server);
}
