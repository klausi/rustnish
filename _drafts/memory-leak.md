---
title: "Testing memory leaks in Rust"
layout: post
---

Rust has many built-in concepts for memory safety, but it cannot prevent application level logic errors that take up system memory. An example would be a server application that stores something for each incoming request in a growing collection or list. If the program does not clean up the growing list then it will take up more and more server memory - thereby exposing aq memory leak.

While working on my reverse proxy project I discovered such a [leak in the HTTP library Hyper](https://github.com/hyperium/hyper/issues/1315). In order to prevent and detect memory leaks in the future I set out my goal 7:

> Add an integration test that ensures that the proxy server is not leaking
> memory (growing RAM usage without shrinking again). Use /proc information to
> compare memory usage of the current process before and after the test.

## Finding memory leaks manually first

A very primitive way of inspecting the memory usage of a program is `ps` on Linux. First we start our Rustnish reverse proxy:

```
cargo run --release
```

Then get the memory information from ps for rustnish in a new terminal:

```
$ ps aux | grep '[r]ustnish'
klausi    3840  0.0  0.0  38504  7832 pts/0    Sl+  17:56   0:00 target/release/rustnish
```

The 6th column is the resident memory usage in kilobytes. Which means our server process is taking up ~8MB in RAM right now.

Now we want to see how our server is doing after it had to deal with a lot of requests. A tool for that is Apache Bench, which is used for load testing on servers. Installation on Ubuntu for example:

```
sudo apt-get install apache2-utils
```

Then fire 1 million requests at our reverse proxy, 4 requests concurrently:

```
ab -c 4 -n 1000000 http://localhost:9090/
```

Now running ps again:

```
$ ps aux | grep '[r]ustnish'
klausi    3840 47.8  3.6 304836 283588 pts/0   Sl+  18:15   2:04 target/release/rustnish
```

Wow, the 6th column is now showing 283,588KB which is ~278MB, something is definitely very wrong here!

Luckily I could track down the problem pretty quick to the Hyper library and after reporting it to the author he committed a fix. Thanks Sean McArthur!

## Automating a memory leak test

Now that the memory leak is fixed we want to make sure it does not happen again. We can setup an integration test that runs on Travis CI whenever code is changed. The strategy for such a test is similar to what we did manually:

1. Start the reverse proxy.
2. Measure the memory footprint.
3. Make a large amount of requests against the proxy, similar to what Apache Bench does.
4. Measure the memory footprint again.
5. Assert that memory usage is below a certain threshold.

The biggest problem is that Rust has no built-in function to get memory usage information of the current program (in PHP there is for example [`memory_get_usage()`](http://php.net/manual/en/function.memory-get-usage.php)). The closest thing is the [procinfo](https://crates.io/crates/procinfo) crate, which uses memory information from /proc on Linux. This is of course platform dependent and can for example not work on MacOS or Windows.

The full test can be found in [memory_leaks.rs](https://github.com/klausi/rustnish/blob/goal-07/tests/memory_leaks.rs).

Getting the current memory usage (resident number of kilobytes in RAM):

```rust
extern crate procinfo;
let memory_before = procinfo::pid::statm_self().unwrap().resident;
```

Emulating Apache Bench and sending 30K requests, 4 at a time:

```rust
let mut core = Core::new().unwrap();
let client = Client::new(&core.handle());

let url: Uri = ("http://127.0.0.1:".to_string() + &port.to_string())
    .parse()
    .unwrap();

let nr_requests = 30_000;
let concurrency = 4;

let mut parallel = Vec::new();
for _i in 0..concurrency {
    let requests_til_done = loop_fn(0, |counter| {
        let mut request = Request::new(Method::Get, url.clone());
        request.set_version(hyper::HttpVersion::Http10);
        client
            .request(request)
            .then(move |_| -> Result<_, hyper::Error> {
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
```

We are building 4 loop futures here with the [`loop_fn()`](https://docs.rs/futures/*/futures/future/fn.loop_fn.html) construct, each iteration sending one request. The 4 futures are executed in parallel and we wait with a [`join_all()`](https://docs.rs/futures/*/futures/future/fn.join_all.html) future until they all are finished.

Note that this is test code, that's why there are lots of `unwrap()` because we don't care about errors (I have [written about `unwrap()` before]({{ site.baseurl }}{% post_url 2017-08-16-replacing-unwrap-and-avoiding-panics-in-rust %})).

As always the hardest part about assembling futures is to get the type spaghetti right. `loop_fn()` has 4 (!!!) generic type parameters, so writing and reasoning about it takes quite some time.