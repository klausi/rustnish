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
use hyper::client::HttpConnector;
use hyper::client::ResponseFuture;
use hyper::header::HeaderName;
use hyper::header::SERVER;
use hyper::server::conn::Http;
use hyper::service::Service;
use hyper::Client;
use hyper::StatusCode;
use hyper::Version;
use hyper::{Body, Request, Response};
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

type DirectResponse = FutureResult<Response<Body>, hyper::Error>;
type UpstreamResponse = futures::Then<
    ResponseFuture,
    FutureResult<Response<Body>, hyper::Error>,
    fn(std::result::Result<Response<Body>, hyper::Error>)
        -> FutureResult<Response<Body>, hyper::Error>,
>;

impl Service for Proxy {
    type ReqBody = Body;
    type ResBody = Body;
    type Error = hyper::Error;
    type Future = Either<DirectResponse, UpstreamResponse>;

    fn call(&mut self, mut request: Request<Body>) -> Self::Future {
        let upstream_uri = {
            // 127.0.0.1 is hard coded here for now because we assume that upstream
            // is on the same host. Should be made configurable later.
            let mut upstream_uri = format!(
                "http://127.0.0.1:{}{}",
                self.upstream_port,
                request.uri().path()
            );
            if let Some(query) = request.uri().query() {
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
                        Response::builder()
                            .status(StatusCode::BAD_REQUEST)
                            .body("Invalid upstream URI".into())
                            .unwrap(),
                    ));
                }
            }
        };

        *request.uri_mut() = upstream_uri;

        {
            let headers = request.headers_mut();
            headers.append(
                HeaderName::from_static("x-forwarded-for"),
                self.source_address.ip().to_string().parse().unwrap(),
            );
            headers.append(
                HeaderName::from_static("x-forwarded-port"),
                self.port.to_string().parse().unwrap(),
            );
        }

        Either::B(self.client.request(request).then(|result| {
            let our_response = match result {
                Ok(mut response) => {
                    let version = match response.version() {
                        Version::HTTP_09 => "0.9",
                        Version::HTTP_10 => "1.0",
                        Version::HTTP_11 => "1.1",
                        Version::HTTP_2 => "2.0",
                    };
                    {
                        let mut headers = response.headers_mut();

                        headers.append(
                            HeaderName::from_static("via"),
                            format!("{} rustnish-0.0.1", version).parse().unwrap(),
                        );

                        // Append a "Server" header if not already present.
                        if !headers.contains_key(SERVER) {
                            headers.insert(SERVER, "rustnish".parse().unwrap());
                        }
                    }

                    response
                }
                Err(_) => {
                    // For security reasons do not show the exact error to end users.
                    // @todo Log the error.
                    Response::builder()
                        .status(StatusCode::BAD_GATEWAY)
                        .body("Something went wrong, please try again later.".into())
                        .unwrap()
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
            let client = Client::new();
            let http = Http::new();

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
