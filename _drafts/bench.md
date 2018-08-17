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

