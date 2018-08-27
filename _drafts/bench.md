---
title: "Benchmarking a Rust web application"
layout: post
---

Performance testing is an important part when developing a network application - you want to know when you have a regression in request throughput in your service.

I set out out my goal 9 for Rustnish:

> Write benchmark code that compares runtime performance of Rustnish against
[Varnish](https://varnish-cache.org/). Use `cargo bench` to execute the benchmarks.

The basic idea of a performance test here is to send many HTTP requests to the web service (the reverse proxy in this case) and measure how fast the responses arrive back. Comparing the results from Rustnish and Varnish should give us an idea if our performance expectations are holding up.

## Manual performance testing with ApacheBench

A quick way to get performance feedback is to run `ab` against our reverse proxy. Start the server in release mode (optimized):

```
cargo run --release
   Compiling rustnish v0.0.1 (file:///home/klausi/workspace/rustnish)
    Finished release [optimized] target(s) in 6.02s
     Running `target/release/rustnish`
Listening on http://127.0.0.1:9090
```

As backend service I'm using the default Ubuntu Apache webserver running on port 80. It delivers a static default test page.

Benchmarking by sending 10k requests, 100 in parallel:

```
$ ab -c 100 -n 10000 http://127.0.0.1:9090/
This is ApacheBench, Version 2.3 <$Revision: 1807734 $>
...
Benchmarking 127.0.0.1 (be patient)
...
Concurrency Level:      100
Time taken for tests:   1.011 seconds
Complete requests:      10000
Failed requests:        0
Total transferred:      116200000 bytes
HTML transferred:       113210000 bytes
Requests per second:    9893.12 [#/sec] (mean)
Time per request:       10.108 [ms] (mean)
Time per request:       0.101 [ms] (mean, across all concurrent requests)
Transfer rate:          112263.78 [Kbytes/sec] received
...
```

That looks quite OK!

Of course it is easy for our reverse proxy to reach this throughput: it does not do anything except passing requests through and adding its own header.

Now we install Varnish on Ubuntu:

```
sudo apt install varnish
```

We configure it to do the sane thing as Rustnish, just passing all requests through. /etc/varnish/default.vcl:

```
vcl 4.0;

# Default backend definition. Set this to point to your content server.
backend default {
    .host = "127.0.0.1";
    .port = "80";
}

sub vcl_recv {
    return (pass);
}
```

And benchmark it:

```
$ ab -c 100 -n 10000 http://127.0.0.1:6081/
This is ApacheBench, Version 2.3 <$Revision: 1807734 $>
...
Benchmarking 127.0.0.1 (be patient)
...
Concurrency Level:      100
Time taken for tests:   1.182 seconds
Complete requests:      10000
Failed requests:        0
Total transferred:      116553545 bytes
HTML transferred:       113210000 bytes
Requests per second:    8458.46 [#/sec] (mean)
Time per request:       11.822 [ms] (mean)
Time per request:       0.118 [ms] (mean, across all concurrent requests)
Transfer rate:          96275.68 [Kbytes/sec] received
```

As you can see Varnish performs slightly worse than Rustnish - which means we are on the right track! Of course Varnish has a much bigger code base with much more complexity compared to our most basic reverse proxy that just passes requests through. This difference is to be expected.


## Automating benchmarks in code

While manual testing is fine we want to automate multiple benchmark scenarios into a benchmark suite that can be executed quickly in one go. `cargo bench` can help us with that - [the unstable Rust book describes what you need to do to use it](https://doc.rust-lang.org/stable/unstable-book/library-features/test.html).

The book has some good points of advice, one point that we are going to deliberately ignore:

> Make the code in the iter loop do something simple, to assist in pinpointing performance improvements (or regressions)

But we want to do a full black box performance test of our service here, so our benchmark will be an HTTP client that sends requests and measures response times. This is not a trivial thing to do with Hyper because there are no examples guides of how to send requests in parallel. Here is a helper function I came up with:

```rust
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
```

Now we can define bench scenarios that should be measured, for example:

```rust
#[bench]
fn c_100_requests(b: &mut test::Bencher) {
    bench_requests(b, 100, 1, 9090);
}

#[bench]
fn c_100_requests_varnish(b: &mut test::Bencher) {
    bench_requests(b, 100, 1, 6081);
}
```

The full source code with the scenarios can be found in the [goal-09 branch](https://github.com/klausi/rustnish/blob/goal-09/benches/rustnish_vs_varnish.rs).

Before this benchmark can be executed we need Varnish running on port 6081 (default) and we need to start a dummy backend and our proxy server:

```
cargo run --release --example hello_9091
cargo run --release --example rustnish_9090
```

Executing `cargo bench` then gives us this:

```
running 12 tests
test a_1_request                       ... bench:     364,246 ns/iter (+/- 103,690)
test a_1_request_varnish               ... bench:     389,026 ns/iter (+/- 63,051)
test b_10_requests                     ... bench:   1,874,980 ns/iter (+/- 377,843)
test b_10_requests_varnish             ... bench:   2,152,368 ns/iter (+/- 356,510)
test c_100_requests                    ... bench:  17,507,140 ns/iter (+/- 2,847,238)
test c_100_requests_varnish            ... bench:  21,896,708 ns/iter (+/- 5,546,318)
test d_10_parallel_requests            ... bench:   1,646,869 ns/iter (+/- 228,179)
test d_10_parallel_requests_varnish    ... bench:   2,012,392 ns/iter (+/- 426,878)
test e_100_parallel_requests           ... bench:   8,508,973 ns/iter (+/- 361,317)
test e_100_parallel_requests_varnish   ... bench:   9,574,347 ns/iter (+/- 764,147)
test f_1_000_parallel_requests         ... bench:  82,898,926 ns/iter (+/- 1,037,534)
test f_1_000_parallel_requests_varnish ... bench:  86,922,588 ns/iter (+/- 1,687,902)
```

Cool, that shows our proxy always being slightly faster than Varnish.