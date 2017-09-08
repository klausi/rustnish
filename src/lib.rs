extern crate hyper;
extern crate futures;
extern crate tokio_core;
#[macro_use]
extern crate error_chain;

use hyper::Client;
use hyper::server::{Http, Request, Response, Service};
use tokio_core::net::TcpListener;
use tokio_core::reactor::Core;
use futures::{Future, Stream};
use futures::future::{Either, FutureResult};
use hyper::client::HttpConnector;
use hyper::client::FutureResponse;
use hyper::header::Host;
use hyper::StatusCode;
use std::sync::mpsc;
use std::thread;
use errors::*;
use hyper::HttpVersion;

mod errors {
    // Create the Error, ErrorKind, ResultExt, and Result types
    error_chain!{}
}

struct Proxy {
    port: u16,
    upstream_port: u16,
    client: Client<HttpConnector>,
}

type DirectResponse = FutureResult<Response, hyper::Error>;
type UpstreamResponse = futures::Then<
    FutureResponse,
    FutureResult<Response, hyper::Error>,
    fn(std::result::Result<Response, hyper::Error>)
       -> FutureResult<Response, hyper::Error>,
>;

impl Service for Proxy {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = Either<DirectResponse, UpstreamResponse>;

    fn call(&self, mut request: Request) -> Self::Future {
        let host = match request.headers().get::<Host>() {
            None => {
                return Either::A(futures::future::ok(
                    Response::new()
                        .with_status(StatusCode::BadRequest)
                        .with_body("No host header in request"),
                ));

            }
            // Copy the string out of the request to avoid borrow checker
            // immutability errors later.
            Some(h) => h.hostname().to_owned(),
        };

        // Copy the request URI out of the request to avoid borrow checker
        // immutability errors later.
        let request_uri = request.uri().to_owned();
        let upstream_string_uri = "http://".to_string() + &host + ":" +
            &self.upstream_port.to_string() + request_uri.path();

        let upstream_uri = match upstream_string_uri.parse() {
            Ok(u) => u,
            _ => {
                // We can't actually test this because parsing the URI never
                // fails. However, should that change at any point this is the
                // right thing to do.
                return Either::A(futures::future::ok(
                    Response::new()
                        .with_status(StatusCode::BadRequest)
                        .with_body("Invalid host header in request"),
                ));
            }
        };

        request.set_uri(upstream_uri);

        if let Some(socket_address) = request.remote_addr() {
            let headers = request.headers_mut();
            headers.append_raw("X-Forwarded-For", socket_address.ip().to_string());
            headers.append_raw("X-Forwarded-Port", self.port.to_string());
        };

        Either::B(self.client.request(request).then(|result| {
            let our_response = match result {
                Ok(response) => {
                    let mut headers = response.headers().clone();
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
                    headers.append_raw("Via", format!("{} rustnish-0.0.1", version));
                    response.with_headers(headers)
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
    let thread = start_server_background(port, upstream_port).chain_err(
        || "Spawning server thread failed",
    )?;
    match thread.join() {
        Ok(thread_result) => {
            match thread_result {
                Ok(_) => bail!("The server thread finished unexpectedly"),
                Err(error) => {
                    Err(Error::with_chain(
                        error,
                        "The server thread stopped with an error",
                    ))
                }
            }
        }
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
    let (ready_tx, ready_rx) = mpsc::channel();

    let thread = thread::Builder::new()
        .name("rustnish".to_owned())
        // Even if our thread returns nothing but errors the Rust convention is
        // to always do that in the form of a Result type.
        .spawn(move || -> Result<()> {
            let address = ([127, 0, 0, 1], port).into();

            // Prepare a Tokio core that we will use for our server and our
            // client.
            let mut core = Core::new().chain_err(|| "Failed to create Tokio core")?;
            let handle = core.handle();
            let http = Http::new();
            let listener = TcpListener::bind(&address, &handle)
                .chain_err(|| format!("Failed to bind server to address {}", address))?;
            let client = Client::new(&handle);

            let server = listener.incoming().for_each(move |(sock, addr)| {
                http.bind_connection(
                    &handle,
                    sock,
                    addr,
                    Proxy {
                        port: port,
                        upstream_port: upstream_port,
                        client: client.clone(),
                    },
                );
                Ok(())
            });
            ready_tx.send(true).chain_err(|| "Failed to send back thread ready signal.")?;

            println!("Listening on http://{}", address);
            core.run(server).chain_err(|| "Tokio core run failed")?;
            bail!("The Tokio core run ended unexpectedly");
        })
        .chain_err(|| "Spawning server thread failed")?;

    // Whether our channel to the thread received something or was closed with
    // an error because the thread errored - we don't care. This call is just
    // here to block until the server binding in the thread is done.
    let _bind_ready = ready_rx.recv();

    Ok(thread)
}
