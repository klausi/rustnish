---
title: "Crashing a Rust Hyper server with a Denial of Service attack"
layout: post
---

I'm writing a reverse proxy in Rust using [Hyper](https://hyper.rs/) and I want to measure performance a bit to know if I'm doing something terribly wrong. By doing that I discovered a Denial of Service vulnerability in Hyper when IO errors are not properly handled. Note that [a workaround has been released in the meantime in Hyper 0.11.20](https://github.com/hyperium/hyper/releases/tag/v0.11.20), more background info can be found in [this Hyper issue](https://github.com/hyperium/hyper/issues/1358).

## A vulnerable Hello world server example

Let's look at the simplest Hyper server example that just serves "Hello world" HTTP responses ([source](https://github.com/hyperium/hyper/blob/v0.11.19/examples/hello.rs)):

```rust
static PHRASE: &'static [u8] = b"Hello World!";

fn main() {
    let addr = ([127, 0, 0, 1], 3000).into();

    let new_service = const_service(service_fn(|_| {
        Ok(Response::<hyper::Body>::new()
            .with_header(ContentLength(PHRASE.len() as u64))
            .with_header(ContentType::plaintext())
            .with_body(PHRASE))
    }));

    let server = Http::new().bind(&addr, new_service).unwrap();
    println!("Listening on http://{} with 1 thread.", server.local_addr().unwrap());
    server.run().unwrap();
}
```

The last call to `server.run()` will block and the program will continue to run until terminated. At least that is what we expect to happen here.

This example is included with the Hyper library and you can run the vulnerable version directly from there:

```
git clone --branch v0.11.19 https://github.com/hyperium/hyper.git
cd hyper
cargo run --example hello
```

## Using ApacheBench to attack the server

My go to tool for load testing is [ApacheBench](https://httpd.apache.org/docs/2.4/programs/ab.html), a simple command line tool for HTTP request testing. I played a bit with the command line options and made the number of concurrent requests a bit too high by mistake:

```
$ ab -r -c 10000 -n 1000000 http://127.0.0.1:3000/
Benchmarking 127.0.0.1 (be patient)
socket: Too many open files (24)
```

Ah yes, 10k requests in parallel will probably not work because the `ab` process is only allowed to open a certain amount of ports. Let's check the limits for a Linux process running under my user account:

```
$ ulimit -a
core file size          (blocks, -c) 0
data seg size           (kbytes, -d) unlimited
scheduling priority             (-e) 0
file size               (blocks, -f) unlimited
pending signals                 (-i) 30562
max locked memory       (kbytes, -l) 64
max memory size         (kbytes, -m) unlimited
open files                      (-n) 1024
pipe size            (512 bytes, -p) 8
POSIX message queues     (bytes, -q) 819200
real-time priority              (-r) 0
stack size              (kbytes, -s) 8192
cpu time               (seconds, -t) unlimited
max user processes              (-u) 30562
virtual memory          (kbytes, -v) unlimited
file locks                      (-x) unlimited
```

Only 1024 open files/ports allowed.

When I checked back on my Hyper server I was surprised to find it dead for the same reason:

```
Listening on http://127.0.0.1:3000 with 1 thread.
thread 'main' panicked at 'called `Result::unwrap()` on an `Err` value: Io(Os {
code: 24, kind: Other, message: "Too many open files" })', libcore/result.rs:945:5
note: Run with `RUST_BACKTRACE=1` for a backtrace.
```

Oops, that is not good. A HTTP server should not just exit when a flood of HTTP requests comes in. It needs to be resilient and keep running at all times. You could argue that the open file limit simply must be configured to a higher value for production use. That way the problem can be postponed to even larger request volumes, but then the problem is the same: the server will abort and die.

## A naive fix with a loop

```rust
let addr = ([127, 0, 0, 1], 3000).into();

loop {
    let new_service = const_service(service_fn(|_| {
        Ok(Response::<hyper::Body>::new()
            .with_header(ContentLength(PHRASE.len() as u64))
            .with_header(ContentType::plaintext())
            .with_body(PHRASE))
    }));

    let server = Http::new()
        .bind(&addr, new_service)
        .unwrap();
    println!("Listening on http://{} with 1 thread.", server.local_addr().unwrap());
    if let Err(e) = server.run() {
        println!("Error: {:?}", e);
    }
}
```

This "works" in the sense that the server does not die and just restarts itself. The problem with this approach is that other connections are dropped when an IO error occurs, causing a service interruption.

## The fix in Hyper

In order to fix this in Hyper itself I contributed [`sleep_on_errors()`](https://docs.rs/hyper/0.11.22/hyper/server/struct.Http.html#method.sleep_on_errors). Starting a HTTP server with that setting will swallow IO errors internally and library users do not have to worry about it. In the case of "Too many open files" errors the server will just wait 10ms before trying to accept the TCP connection again, hoping that free ports have become available in the meantime.

This setting is currently (Hyper v0.11.22) disabled by default and you must enable it like this:

```rust
let server = Http::new()
    .sleep_on_errors(true)
    .bind(&addr, new_service)
    .unwrap();
println!("Listening on http://{} with 1 thread.", server.local_addr().unwrap());
server.run().unwrap();
```

Future versions of Hyper (probably starting with 0.12.x) will enable this setting per default to have a better developer experience. Progress is tracked in [this issue](https://github.com/hyperium/hyper/issues/1455).

Thanks a lot to Paul Colomiets (the fix was copied from their [tk-listen](https://github.com/tailhook/tk-listen) library) and Sean McArthur for helping me understand and fix this problem!

## Conclusion

Coming from a PHP background I'm not used to thinking about or handling IO errors. That is all handled by well tested web servers like Apache and Nginx, while I only care about application specific code in PHP. Using a low level library such as Hyper exposes more than just request/response handling. Maybe using a higher level framework such as [Rocket](https://rocket.rs/) even for the most basic use case (such as my proxy) is a safer choice.

I think that a HTTP server API such as Hyper should be secure by default and prevent server exits where possible. We will get there hopefully!
