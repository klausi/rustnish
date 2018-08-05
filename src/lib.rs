#[macro_use]
extern crate error_chain;
extern crate futures;
extern crate hyper;
extern crate num_cpus;
extern crate tokio_core;

use errors::ResultExt;
use errors::*;
use futures::future::{Either, FutureResult};
use futures::{Future, Stream};
use hyper::client::FutureResponse;
use hyper::client::HttpConnector;
use hyper::server::{Http, Request, Response, Service};
use hyper::Client;
use hyper::HttpVersion;
use hyper::StatusCode;
use std::net::SocketAddr;
use std::thread;
use tokio_core::net::TcpListener;
use tokio_core::reactor::Core;

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
    let thread = start_server_background(port, upstream_port)
        .chain_err(|| "Spawning server thread failed")?;
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
            let mut core = Core::new().unwrap();
            let handle = core.handle();

            // We can't use Http::new().bind() because we need to pass down the
            // remote source IP address to our proxy service. So we need to
            // create a TCP listener ourselves and handle each connection to
            // have access to the source IP address.
            // @todo Simplify this once Hyper has a better API to handle IP
            // addresses.
            let listener = TcpListener::bind(&address, &handle)
                .chain_err(|| format!("Failed to bind server to address {}", address))?;
            let client = Client::new(&handle);
            let mut http = Http::<hyper::Chunk>::new();
            // Let Hyper swallow IO errors internally to keep the server always
            // running.
            http.sleep_on_errors(true);

            let server = listener.incoming().for_each(move |(socket, source_address)| {
                handle.spawn(
                    http.serve_connection(socket, Proxy {
                            port,
                            upstream_port,
                            client: client.clone(),
                            source_address,
                        })
                        .map(|_| ())
                        .map_err(|_| ())
                );
                Ok(())
            });

            ready_tx.send(true).chain_err(|| "Failed to send back thread ready signal.")?;

            println!("Listening on http://{}", address);
            core.run(server).chain_err(|| "Tokio core run error")?;
            bail!("The Tokio core run ended unexpectedly");
        })
        .chain_err(|| "Spawning server thread failed")?;

    // Whether our channel to the thread received something or was closed with
    // an error because the thread errored - we don't care. This call is just
    // here to block until the server binding in the thread is done.
    let _bind_ready = ready_rx.recv();

    Ok(thread)
}
