---
title: "Converting a Hyper server to Tokio"
layout: post
---

Since my [first blog post where I constructed a server with Hyper]({{
site.baseurl }}{% post_url 2017-04-30-getting-started-with-rust %}) some time
has passed and there is now a new version of the library that is based on
[Tokio](https://tokio.rs). My goal 3:

> A new version of the Hyper library has been released which is
> based on the Tokio library. Convert the existing code to use that new version
> and provide one integration test case.

Tokio handles input/output asynchronously, which makes setting up a server more
complicated. The benefit is more efficient parallel execution with a
non-blocking event loop.

You can find all the code in [the goal-03 branch on
Github](https://github.com/klausi/rustnish/tree/goal-03).

## Upgrading Hyper

Hyper is already registered in the Cargo.toml file as a project dependency, so
there is only one step to update:

```
cargo update
```

This will download the new Hyper library version and change the version number
in Cargo.lock.

## Converting Handler to Service

Old code:

```rust
struct Proxy {
    upstream_port: u16,
}

impl Handler for Proxy {
    fn handle(&self, request: Request, response: Response) {
        // Function body omitted here.
    }
}
```

New code:

```rust
struct Proxy {
    upstream_port: u16,
    client: Client<HttpConnector>,
}

impl Service for Proxy {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = Either<FutureResult<Self::Response, Self::Error>,
        FutureResponse>;

    fn call(&self, request: Request) -> Self::Future {
        // Function body omitted here.
    }
}
```

The first thing you'll notice is that the Proxy struct has an additional field
to hold a Hyper client instance. This is a bit of an implementation detail of
my specific reverse proxy use case here. Since I'm using a Hyper server and a
Hyper client at the same time in my program I want them to run on the same
Tokio core (the event loop). Which means that I need to pre-construct my HTTP
client and "inject" it into my Proxy service.

The types of the Service determine what kind of requests and responses go in
and out of it.

## A word on Futures

A Future is a result of an operation that will be available later. You can
think of callbacks or the concept of a Promise in JavaScript. Execution is
non-blocking:

* In the old ```handle()``` function the execution time is directly spent there
assembling and preparing the response and returning it once everything is done.
* The new ```call()``` function runs through more quickly. Anything that
requires further input/output (fetching with the HTTP client in our case) is
postponed as Future and the function returns immediately.

The hardest part is to get the Future type right. In our case we can have 2
different kinds of responses: direct responses if the client request is wrong
in any way and upstream responses that come out of our HTTP client invocation.
We can use the ```Either``` helper Future to combine those 2 Future types.

## Starting a server and sharing a Tokio core

Old code:

```rust
pub fn start_server(port: u16, upstream_port: u16) -> Listening {
    let address = "127.0.0.1:".to_owned() + &port.to_string();
    let server = Server::http(&address).unwrap();
    let listening = server
        .handle(Proxy { upstream_port: upstream_port })
        .unwrap();
    println!("Listening on {}", address);

    listening
}

fn main() {
    let port: u16 = 9090;
    let upstream_port: u16 = 80;
    let _listening = rustnish::start_server(port, upstream_port);
}
```

New code:

```rust
pub fn start_server(port: u16, upstream_port: u16) -> thread::JoinHandle<()> {

    let thread = thread::Builder::new()
        .name("rustnish".to_owned())
        .spawn(move || {
            let address = "127.0.0.1:".to_owned() + &port.to_string();
            println!("Listening on http://{}", address);
            let addr = address.parse().unwrap();

            // Prepare a Tokio core that we will use for our server and our
            // client.
            let mut core = Core::new().unwrap();
            let handle = core.handle();
            let http = Http::new();
            let listener = TcpListener::bind(&addr, &handle).unwrap();
            let client = Client::new(&handle);

            let server = listener
                .incoming()
                .for_each(move |(sock, addr)| {
                    http.bind_connection(&handle,
                                         sock,
                                         addr,
                                         Proxy {
                                             upstream_port: upstream_port,
                                             client: client.clone(),
                                         });
                    Ok(())
                });

            core.run(server).unwrap();
        })
        .unwrap();

    thread
}

fn main() {
    let port: u16 = 9090;
    let upstream_port: u16 = 80;
    let thread = rustnish::start_server(port, upstream_port);
    let _guard = thread.join();
}
```

So we went from 15 lines of code to 40 lines of code. What happened?

1. ```core.run(server)``` is starting the event loop and blocking. That's why
we need to set up our own thread handling. Inspired by [Hyper test
code](https://github.com/hyperium/hyper/blob/master/tests/server.rs#L583).
2. The Hyper server would create its own internal Tokio core event loop when
using ```http.bind()```. But we need our event loop beforehand to initialize
our HTTP client. That's why we need the complicated setup with
```Core::new()``` and ```TcpListener``` and ```http.bind_connection()``` to
pass in an existing event loop handle.
3. We want to return something useful (non-blocking) to the consumer that calls
our ```start_server()``` function. We have spawned a thread so our ```main()```
consumer can just wait indefinitely on the thread by calling ```join()```.

## Converting the response handling

This is where the new version of the Hyper library shines. The request and
response types are now unified: a HTTP client response is the same as a HTTP
server response! This is very useful in our reverse proxy use case where we can
just pass through responses as is.

I'm omitting [my old Hyper
code](https://github.com/klausi/rustnish/blob/goal-02/src/lib.rs#L35) here
because it is quite convoluted and long. The new code is so much nicer:

```rust
impl Service for Proxy {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = Either<FutureResult<Self::Response, Self::Error>,
        FutureResponse>;

    fn call(&self, request: Request) -> Self::Future {
        let host = match request.headers().get::<Host>() {
            None => {
                return Either::A(futures::future::ok(Response::new()
                    .with_status(StatusCode::BadRequest)
                    .with_body("No host header in request")));
            }
            Some(h) => h.hostname(),
        };

        let request_uri = request.uri();
        let upstream_uri = ("http://".to_string() + host + ":"
            + &self.upstream_port.to_string() + request_uri.path())
            .parse()
            .unwrap();

        Either::B(self.client.get(upstream_uri))
    }
}
```

In the first part of ```call()``` we quickly build a custom HTTP response when
there is no HTTP Host header on the incoming request. The real magic happens on
the last line: we invoke the HTTP client to make a GET request and we can use
the resulting Future verbatim as is as our server response. The GET request is
spawned on the event loop, a Future is returned immediately and our
```call()``` function returns. The Future is passed back and as soon as it
evaluates (the GET request is done) the response is sent out as server response.

## Converting integration tests

The integration testing experience has changed in good and bad ways:

* In [my old integration tests]({{ site.baseurl }}{% post_url
2017-05-25-writing-integration-tests-in-rust %}) I was [struggling with hanging
test cases on
panics](https://users.rust-lang.org/t/how-do-you-write-integration-tests-that-fail-early-and-often/11297)
and not being able to tear down test services
reliably. This problem has never occurred in [the new integration
test](https://github.com/klausi/rustnish/blob/goal-03/tests/integration_tests.rs
) because everything is shut down as it should be when the variables run out of
scope in the test function. I think that is exactly the Rust way of cleaning
up, so yay!
* The same boilerplate of thread handling and Tokio core setup is needed when
creating quick and dirty HTTP servers and clients for testing. There are no
synchronous helper constructs to shortcut this in test code, so you need to
invent those helpers yourself for your integration test.

## Conclusion

The new Hyper library forces you to think more about where your HTTP server is
blocking and it also forces a basic understanding of asynchronous programming
and the concept of Futures. Once that obstacle of learning is out of the way
and all the boilerplate of thread handling, Tokio core and Future types are set
up the rest of your server implementation falls into place nicely.
