extern crate hyper;

use hyper::Client;
use hyper::server::{Server, Request, Response};
use std::io;

fn main() {
    let server = Server::http("127.0.0.1:9090").unwrap();
    // If a function returns something in Rust you can't ignore it, so we need this superflous
    // unused variable here. Starting it with "_" tells the compiler to ignore it.
    let _guard = server.handle(pipe_through);
    println!("Listening on http://127.0.0.1:9090");
}

fn pipe_through(request: Request, mut response: Response) {
    let client = Client::new();
    // I think the upstream response needs to be mutable because we consume the body further down?
    let mut upstream_response = client.get("http://drupal-8.localhost/").send().unwrap();
    *response.status_mut() = upstream_response.status;
    // Cloning is quite useless here, we actually just want to move the headers. But how?
    *response.headers_mut() = upstream_response.headers.clone();

    // Forward the body of the upstream response in our response body.
    io::copy(&mut upstream_response, &mut response.start().unwrap()).unwrap();
}
