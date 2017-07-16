---
title: "Converting a Hyper server to Tokio"
layout: post
---

Since my [first blog post where I constructed a server with Hyper]({{ site.baseurl }}{% post_url 2017-04-30-getting-started-with-rust %}) some time has passed and there is now a new version of the library that is based on [Tokio](https://tokio.rs). My goal 2:

> A new version of the Hyper library has been released which is
> based on the Tokio library. Convert the existing code to use that new version
> and provide one integration test case.

Tokio handles input/output asynchronously, which makes setting up a server more complicated. The benefit is more efficient parallel execution with a non-blocking event loop.

## Upgrading Hyper

Hyper is already registered in the Cargo.toml file as a project dependency, so there is only one step to update:

```
cargo update
```

This will download the new Hyper library version and change the version number in Cargo.lock.

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
    type Future = Either<FutureResult<Self::Response, Self::Error>, FutureResponse>;

    fn call(&self, request: Request) -> Self::Future {
        // Function body omitted here.
    }
}
```

The first thing you'll notice is that the Proxy struct has an additional field to hold a Hyper client instance. This is a bit of an implementation detail of my specific reverse proxy use case here. Since I'm using a Hyper server and a Hyper client at the same time in my program I want them to run on the same Tokio core (the event loop). Which means that I need to pre-construct my HTTP client and "inject" it into my Proxy service.

The types of the Service determine what kind of requests and responses go in and out of it.

## A word on Futures

A Future is a callback that will be executed later. Execution is non-blocking:

* In the old ```handle()``` function the execution time is directly spent there assembling and preparing the response and returning it once everything is done.
* The new ```call()``` function runs through more quickly. Anything that requires further input/output (fetching with the HTTP client in our case) is postponed as Future and the function returns immediately.

The hardest part is to get the Future type right. In our case we can have 2 different kinds of responses: direct responses if the client request is wrong in any way and upstream responses that come out of our HTTP client invocation. We can use the ```Either``` helper Future to combine those 2 Future types.

## Starting a server and sharing a Tokio core

Old code:

```rust
pub fn start_server(port: u16, upstream_port: u16) -> Listening {
    let address = "127.0.0.1:".to_owned() + &port.to_string();
    let server = Server::http(&address).unwrap();
    let listening = server
        .handle(ProxyHandler { upstream_port: upstream_port })
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
pub struct Server {
    pub shutdown_signal: Option<oneshot::Sender<()>>,
    pub thread: Option<thread::JoinHandle<()>>,
}

pub fn start_server(port: u16, upstream_port: u16) -> Server {
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    let thread = thread::Builder::new()
        .name("rustnish".to_owned())
        .spawn(move || {
            let address = "127.0.0.1:".to_owned() + &port.to_string();
            println!("Listening on {}", address);
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

    Server {
        shutdown_signal: Some(shutdown_tx),
        thread: Some(thread),
    }
}

fn main() {
    let port: u16 = 9090;
    let upstream_port: u16 = 80;
    let server = rustnish::start_server(port, upstream_port);
    let _guard = server.thread.unwrap().join();
}
```

So we went from 15 lines of code to 50 lines of code. What happened?

1. ```core.run(server)``` is starting the event loop and blocking. That's why we need to set up our own thread handling and external shutdown signaling. Inspired by [Hyper test code](https://github.com/hyperium/hyper/blob/master/tests/server.rs#L583).
2.