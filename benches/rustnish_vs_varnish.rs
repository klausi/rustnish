#![feature(test)]

// Performs various timing tests on Rustnish and Varnish.
//
// To better compare Varnish and Rustnish they must be started both externally
// so that the benchmark code here only executes HTTP client timing code.
//
// The backend service must be started with `cargo run --example hello`.
//
// Rustnish must be started with `cargo run --release --example rustnish_9090`.
//
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
// Execute with `cargo bench`.

extern crate futures;
extern crate hyper;
extern crate test;
extern crate tokio_core;

use futures::future::{join_all, loop_fn, Loop};
use futures::{Future, Stream};
use tokio_core::reactor::Core;

#[bench]
fn a_1_request(b: &mut test::Bencher) {
    bench_requests(b, 1, 1, 9090);
}

#[bench]
fn a_1_request_varnish(b: &mut test::Bencher) {
    // Assume Varnish is already running.
    bench_requests(b, 1, 1, 6081);
}

#[bench]
fn b_10_requests(b: &mut test::Bencher) {
    bench_requests(b, 10, 1, 9090);
}

#[bench]
fn b_10_requests_varnish(b: &mut test::Bencher) {
    // Assume Varnish is already running.
    bench_requests(b, 10, 1, 6081);
}

#[bench]
fn c_100_requests(b: &mut test::Bencher) {
    bench_requests(b, 100, 1, 9090);
}

#[bench]
fn c_100_requests_varnish(b: &mut test::Bencher) {
    // Assume Varnish is already running.
    bench_requests(b, 100, 1, 6081);
}

#[bench]
fn d_10_parallel_requests(b: &mut test::Bencher) {
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
    // Initialize all the Tokio runtime stuff.
    let mut core = Core::new().unwrap();
    let handle = core.handle();
    let client = hyper::Client::new(&handle);

    // Target is localhost with the port of the proxy under test.
    let url: hyper::Uri = format!("http://127.0.0.1:{}/get", proxy_port)
        .parse()
        .unwrap();

    // This is the benchmark loop that will be executed multiple times and
    // measured.
    b.iter(move || {
        // Build a list of futures that we will execute all at once in parallel
        // in the end.
        let mut parallel = Vec::new();
        for _i in 0..concurrency {
            // A future that sends requests sequentially by scheduling itself in
            // a loop-like way.
            let requests_til_done = loop_fn(0, |counter| {
                client
                    .get(url.clone())
                    .and_then(|res| {
                        assert_eq!(
                            res.status(),
                            hyper::StatusCode::Ok,
                            "Did not receive a 200 HTTP status code. Make sure Varnish is configured on port 6081 and the backend port is set to 9091 in /etc/varnish/default.vcl. Make sure the backend server is running with `cargo run --example hello_9091` and Rustnish with `cargo run --release --example rustnish_9090`.");
                        // Read response body until the end.
                        res.body().for_each(|_chunk| Ok(()))
                    })
                    // Break condition of the future "loop". The return values
                    // signal the loop future if it should run another iteration
                    // or not.
                    .and_then(move |_| {
                        if counter < (amount / concurrency) {
                            Ok(Loop::Continue(counter + 1))
                        } else {
                            Ok(Loop::Break(counter))
                        }
                    })
            });
            parallel.push(requests_til_done);
        }

        // The execution should finish when all futures are done.
        let work = join_all(parallel);
        // Now run it! Up to this point no request has been sent, we just
        // assembled heavily nested futures so far.
        core.run(work).unwrap();
    });
}
