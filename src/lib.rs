#[macro_use]
extern crate error_chain;
extern crate futures;
extern crate hyper;
extern crate num_cpus;
extern crate tokio_core;

use hyper::Client;
use hyper::server::{Http, Request, Response, Service};
use std::net::TcpListener;
use tokio_core::reactor::Core;
use futures::{Future, Stream};
use futures::future::{Either, FutureResult};
use futures::sync::mpsc;
use hyper::client::HttpConnector;
use hyper::client::FutureResponse;
use hyper::StatusCode;
use std::net::SocketAddr;
use std::thread;
use errors::*;
use hyper::HttpVersion;
use tokio_core::net::TcpStream;

mod errors {
    // Create the Error, ErrorKind, ResultExt, and Result types
    error_chain!{}
}

struct Proxy {
    port: u16,
    upstream_port: u16,
    client: Client<HttpConnector>,
    // The socket address the original request is coming from.
    source_address: SocketAddr,
}

type DirectResponse = FutureResult<Response, hyper::Error>;
type UpstreamResponse = futures::Then<
    FutureResponse,
    FutureResult<Response, hyper::Error>,
    fn(std::result::Result<Response, hyper::Error>) -> FutureResult<Response, hyper::Error>,
>;

impl Service for Proxy {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = Either<DirectResponse, UpstreamResponse>;

    fn call(&self, mut request: Request) -> Self::Future {
        let upstream_uri = {
            // 127.0.0.1 is hard coded here for now because we assume that upstream
            // is on the same host. Should be made configurable later.
            let mut upstream_uri = format!(
                "http://127.0.0.1:{}{}",
                self.upstream_port,
                request.uri().path()
            );
            if let Some(query) = request.query() {
                upstream_uri.push('?');
                upstream_uri.push_str(query);
            }
            match upstream_uri.parse() {
                Ok(u) => u,
                _ => {
                    // We can't actually test this because parsing the URI never
                    // fails. However, should that change at any point this is the
                    // right thing to do.
                    return Either::A(futures::future::ok(
                        Response::new()
                            .with_status(StatusCode::BadRequest)
                            .with_body("Invalid upstream URI"),
                    ));
                }
            }
        };

        request.set_uri(upstream_uri);

        {
            let headers = request.headers_mut();
            headers.append_raw("X-Forwarded-For", self.source_address.ip().to_string());
            headers.append_raw("X-Forwarded-Port", self.port.to_string());
        }

        Either::B(self.client.request(request).then(|result| {
            let our_response = match result {
                Ok(mut response) => {
                    let version = match response.version() {
                        HttpVersion::Http09 => "0.9",
                        HttpVersion::Http10 => "1.0",
                        HttpVersion::Http11 => "1.1",
                        HttpVersion::H2 | HttpVersion::H2c => "2.0",
                        // Not sure what we should do when we don't know the
                        // version, this case is probably unreachable code
                        // anyway.
                        _ => "?",
                    };
                    {
                        let mut headers = response.headers_mut();

                        headers.append_raw("Via", format!("{} rustnish-0.0.1", version));

                        // Append a "Server" header if not already present.
                        if !headers.has::<hyper::header::Server>() {
                            headers.set::<hyper::header::Server>(hyper::header::Server::new(
                                "rustnish",
                            ));
                        }
                    }

                    response
                }
                Err(_) => {
                    // For security reasons do not show the exact error to end users.
                    // @todo Log the error.
                    Response::new()
                        .with_status(StatusCode::BadGateway)
                        .with_body("Something went wrong, please try again later.")
                }
            };
            futures::future::ok(our_response)
        }))
    }
}

pub fn start_server_blocking(port: u16, upstream_port: u16) -> Result<()> {
    let thread =
        start_server_background(port, upstream_port).chain_err(|| "Spawning server thread failed")?;
    match thread.join() {
        Ok(thread_result) => match thread_result {
            Ok(_) => bail!("The server thread finished unexpectedly"),
            Err(error) => Err(Error::with_chain(
                error,
                "The server thread stopped with an error",
            )),
        },
        // I would love to pass up the error here, but it is a Box and I don't
        // know how to do that.
        Err(_) => bail!("The server thread panicked"),
    }
}

pub fn start_server_background(
    port: u16,
    upstream_port: u16,
) -> Result<thread::JoinHandle<Result<()>>> {
    // We need to block until the server has bound successfully to the port, so
    // we block on this channel before we return. As soon as the thread sends
    // out the signal we can return.
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();

    let thread = thread::Builder::new()
        .name("rustnish".to_owned())
        // Even if our thread returns nothing but errors the Rust convention is
        // to always do that in the form of a Result type.
        .spawn(move || -> Result<()> {
            let address: SocketAddr = ([127, 0, 0, 1], port).into();

            // Use `std::net` to bind the requested port, we'll use this on the main
            // thread below
            let listener = TcpListener::bind(&address).chain_err(|| format!("Failed to bind server to address {}", address))?;
            let num_cpus = num_cpus::get();
            ready_tx.send(true).chain_err(|| "Failed to send back thread ready signal.")?;
            println!("Listening on http://{} using {} worker threads", address, num_cpus);

            // Spin up our worker threads, creating a channel routing to each worker
            // thread that we'll use below.
            let mut channels = Vec::new();
            for _ in 0..num_cpus {
                let (tx, rx) = mpsc::unbounded();
                channels.push(tx);
                thread::spawn(move || worker(rx, port, upstream_port));
            }

            // Infinitely accept sockets from our `std::net::TcpListener`, as this'll do
            // blocking I/O. Each socket is then shipped round-robin to a particular
            // thread which will associate the socket with the corresponding event loop
            // and process the connection.
            let mut next = 0;
            for socket in listener.incoming() {
                let socket = match socket {
                    Ok(socket) => socket,
                    // Ignore socket errors like "Too many open files" on the OS
                    // level. Just continue with the next request.
                    Err(_) => continue,
                };
                channels[next].unbounded_send(socket).chain_err(|| "worker thread died")?;
                next = (next + 1) % channels.len();
            }

            bail!("The TCP listener stopped unexpectedly");
        })
        .chain_err(|| "Spawning server thread failed")?;

    // Whether our channel to the thread received something or was closed with
    // an error because the thread errored - we don't care. This call is just
    // here to block until the server binding in the thread is done.
    let _bind_ready = ready_rx.recv();

    Ok(thread)
}

// Represents one worker thread of the server that receives TCP connections from
// the main server thread.
fn worker(rx: mpsc::UnboundedReceiver<std::net::TcpStream>, port: u16, upstream_port: u16) {
    let mut core = Core::new().unwrap();
    let handle = core.handle();
    let http = Http::<hyper::Chunk>::new();
    let client = Client::new(&handle);

    let done = rx.for_each(move |socket| {
        // First up when we receive a socket we associate it with our event loop
        // using the `TcpStream::from_stream` API. After that the socket is not
        // a `tokio_core::net::TcpStream` meaning it's in nonblocking mode and
        // ready to be used with Tokio
        let socket = match TcpStream::from_stream(socket, &handle) {
            Ok(socket) => socket,
            Err(error) => {
                println!(
                    "Failed to read TCP stream, ignoring connection. Error: {}",
                    error
                );
                return Ok(());
            }
        };
        let addr = match socket.peer_addr() {
            Ok(addr) => addr,
            Err(error) => {
                println!(
                    "Failed to get remote address, ignoring connection. Error: {}",
                    error
                );
                return Ok(());
            }
        };

        let connection = http.serve_connection(
            socket,
            Proxy {
                port: port,
                upstream_port: upstream_port,
                client: client.clone(),
                source_address: addr,
            },
        ).map(|_| ())
            .map_err(move |err| println!("server connection error: ({}) {}", addr, err));

        handle.spawn(connection);
        Ok(())
    });
    match core.run(done) {
        Ok(_) => println!("Worker tokio core run ended unexpectedly"),
        Err(_) => println!("Worker tokio core run error."),
    };
}
