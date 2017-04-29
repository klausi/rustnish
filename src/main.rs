extern crate hyper;

use hyper::Client;
use hyper::client::IntoUrl;
use hyper::server::{Server, Request, Response};
use hyper::uri::RequestUri;
use hyper::header::Host;
use std::error::Error;
use hyper::status::StatusCode;
use std::io;

fn main() {
    let server = Server::http("127.0.0.1:9090").unwrap();
    // If a function returns something in Rust you can't ignore it, so we need this superflous
    // unused variable here. Starting it with "_" tells the compiler to ignore it.
    let _guard = server.handle(pipe_through);
    println!("Listening on http://127.0.0.1:9090");
}

fn pipe_through(request: Request, mut response: Response) {
    let path = match request.uri {
        RequestUri::AbsolutePath(p) => p,
        RequestUri::AbsoluteUri(url) => url.path().to_string(),
        RequestUri::Authority(p) => p,
        RequestUri::Star => "*".to_string(),
    };
    let host = match request.headers.get::<Host>() {
        None => {
            return error_page("No host header in request".to_string(),
                              StatusCode::BadRequest,
                              response)
        }
        Some(h) => h,
    };
    let hostname = host.hostname.to_string();
    // String concatenation is complicated in Rust. In order to create a new variable which
    // concatenates 3 strings we first have to allocate memory by making the first variable a
    // string.
    let protocol = "http://".to_string();
    let url_string = protocol + &hostname + &path;
    let url = match url_string.into_url() {
        Ok(u) => u,
        Err(e) => {
            return error_page(format!("Error parsing Host header '{}': {}",
                                      url_string,
                                      e.description()),
                              StatusCode::InternalServerError,
                              response)
        }
    };

    // @todo Add proxy config so that requests always go to localhost and this is not an open
    // relay.
    let client = Client::new();

    let request_builder = client
        .request(request.method, url)
        .headers(request.headers.clone());
    let mut upstream_response = request_builder.send().unwrap();
    *response.status_mut() = upstream_response.status;
    // Cloning is quite useless here, we actually just want to move the headers. But how?
    *response.headers_mut() = upstream_response.headers.clone();

    // Forward the body of the upstream response in our response body.
    io::copy(&mut upstream_response, &mut response.start().unwrap()).unwrap();
}

/**
 * Sets an error response.
 */
fn error_page(message: String, http_code: StatusCode, mut response: Response) {
    println!("{}", message);
    *response.status_mut() = http_code;
    // @todo set response body with the message.
}
