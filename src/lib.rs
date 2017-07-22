extern crate hyper;
extern crate futures;
extern crate tokio_core;
#[macro_use]
extern crate error_chain;

use hyper::Client;
use hyper::server::{Http, Request, Response, Service};
use tokio_core::net::TcpListener;
use tokio_core::reactor::Core;
use futures::{Stream, Future};
use futures::future::{Either, FutureResult};
use hyper::client::HttpConnector;
use hyper::client::FutureResponse;
use hyper::header::Host;
use hyper::StatusCode;
use std::sync::mpsc;
use std::thread;
use errors::*;

mod errors {
    // Create the Error, ErrorKind, ResultExt, and Result types
    error_chain!{}
}

struct Proxy {
    upstream_port: u16,
    client: Client<HttpConnector>,
}

impl Service for Proxy {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = Either<
        FutureResult<Self::Response, Self::Error>,
        futures::OrElse<
            FutureResponse,
            FutureResult<Self::Response, Self::Error>,
            fn(Self::Error)
                -> FutureResult<Self::Response, Self::Error>,
        >,
    >;

    fn call(&self, request: Request) -> Self::Future {
        let host = match request.headers().get::<Host>() {
            None => {
                return Either::A(futures::future::ok(
                    Response::new()
                        .with_status(StatusCode::BadRequest)
                        .with_body("No host header in request"),
                ));

            }
            Some(h) => h.hostname(),
        };

        let request_uri = request.uri();
        let upstream_string_uri = "http://".to_string() + host + ":" +
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

        Either::B(self.client.get(upstream_uri).or_else(|_| {
            // For security reasons do not show the exact error to end users.
            // @todo Log the error.
            futures::future::ok(
                Response::new()
                    .with_status(StatusCode::BadGateway)
                    .with_body("Something went wrong, please try again later."),
            )
        }))
    }
}

pub fn start_server_blocking(port: u16, upstream_port: u16) -> Result<()> {
    let thread = start_server_background(port, upstream_port)
        .chain_err(|| "Spawning server thread failed")?;
    match thread.join() {
        Ok(thread_error) => {
            return Err(Error::with_chain(
                thread_error,
                "The server thread stopped unexpectedly",
            ))
        }
        // I would love to pass up the error here, but it is a Box and I don't
        // know how to do that.
        Err(_) => bail!("Blocking on the server thread failed"),
    };
}

pub fn start_server_background(port: u16, upstream_port: u16) -> Result<thread::JoinHandle<Error>> {
    // We need to block until the server has bound successfully to the port, so
    // we block on this channel before we return. As soon as the thread sends
    // out the signal we can return.
    let (ready_tx, ready_rx) = mpsc::channel();

    let thread = thread::Builder::new()
        .name("rustnish".to_owned())
        .spawn(move || -> Error {
            let address = ([127, 0, 0, 1], port).into();

            // Prepare a Tokio core that we will use for our server and our
            // client.
            let mut core = Core::new().unwrap();
            let handle = core.handle();
            let http = Http::new();
            let listener = TcpListener::bind(&address, &handle).unwrap();
            let client = Client::new(&handle);

            let server = listener.incoming().for_each(move |(sock, addr)| {
                http.bind_connection(
                    &handle,
                    sock,
                    addr,
                    Proxy {
                        upstream_port: upstream_port,
                        client: client.clone(),
                    },
                );
                Ok(())
            });
            ready_tx.send(true).unwrap();

            println!("Listening on http://{}", address);
            match core.run(server) {
                Ok(_) => "The Tokio core run ended unexpectedly".into(),
                Err(e) => Error::with_chain(e, "Tokio core run failed"),
            }
        })
        .chain_err(|| "Spawning server thread failed")?;

    let _bind_ready = ready_rx.recv().unwrap();

    Ok(thread)
}
