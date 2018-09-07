#[macro_use]
extern crate error_chain;
extern crate futures;
extern crate hyper;
extern crate tokio;

use errors::ResultExt;
use errors::*;
use futures::{Future, Stream};
use hyper::client::HttpConnector;
use hyper::header::HeaderName;
use hyper::header::{SERVER, VIA};
use hyper::server::conn::Http;
use hyper::service::Service;
use hyper::Client;
use hyper::StatusCode;
use hyper::Version;
use hyper::{Body, Request, Response};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::runtime::Runtime;

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

impl Service for Proxy {
    type ReqBody = Body;
    type ResBody = Body;
    type Error = hyper::Error;
    type Future = Box<Future<Item = Response<Body>, Error = hyper::Error> + Send>;

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
                    return Box::new(futures::future::ok(
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

        Box::new(self.client.request(request).then(|result| {
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

                        headers.append(VIA, format!("{} rustnish-0.0.1", version).parse().unwrap());

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
    let runtime = start_server_background(port, upstream_port)
        .chain_err(|| "Spawning server thread failed")?;

    runtime.shutdown_on_idle().wait().unwrap();

    bail!("The server thread finished unexpectedly");
}

pub fn start_server_background(port: u16, upstream_port: u16) -> Result<Runtime> {
    let address: SocketAddr = ([127, 0, 0, 1], port).into();
    let mut runtime = Runtime::new().unwrap();

    // We can't use Http::new().bind() because we need to pass down the
    // remote source IP address to our proxy service. So we need to
    // create a TCP listener ourselves and handle each connection to
    // have access to the source IP address.
    // @todo Simplify this once Hyper has a better API to handle IP
    // addresses.
    let listener = TcpListener::bind(&address)
        .chain_err(|| format!("Failed to bind server to address {}", address))?;
    let client = Client::new();
    let http = Http::new();

    let server = listener
        .incoming()
        .for_each(move |socket| {
            let source_address = socket.peer_addr().unwrap();
            tokio::spawn(
                http.serve_connection(
                    socket,
                    Proxy {
                        port,
                        upstream_port,
                        client: client.clone(),
                        source_address,
                    },
                ).map(|_| ())
                .map_err(|_| ()),
            );
            Ok(())
        }).map_err(|e| panic!("accept error: {}", e));

    println!("Listening on http://{}", address);
    runtime.spawn(server);

    Ok(runtime)
}
