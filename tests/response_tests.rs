extern crate hyper;
extern crate futures;
extern crate rustnish;
extern crate tokio_core;

use hyper::{Method, StatusCode};
use hyper::header::Host;
use hyper::server::Request;
use futures::{Future, Stream};
use std::str;

mod common;

#[test]
fn test_pass_through() {
    let port = 9090;
    let upstream_port = 9091;

    // Start a dummy server on port 9091 that just echoes the request.
    let _dummy_server = common::start_dummy_server(upstream_port);

    // Start our reverse proxy which forwards to the dummy server.
    let _proxy = rustnish::start_server_background(port, upstream_port);

    // Make a request to the proxy and check if we get the echo back.
    let url = ("http://127.0.0.1:".to_string() + &port.to_string())
        .parse()
        .unwrap();
    let response = common::client_get(url);

    assert_eq!(
        response.headers().get_raw("Via").unwrap(),
        "1.1 rustnish-0.0.1"
    );

    let body = response.body().concat2().wait().unwrap();
    let result = str::from_utf8(&body).unwrap();

    // Check that the request method was GET.
    assert_eq!(
        "Request { method: Get, uri: \"/\", version: Http11, remote_addr:",
        &result[..62]
    );

    // Check that an X-Forwarded-For header was added on the request.
    assert!(result.contains("\"X-Forwarded-For\": \"127.0.0.1\""));

    assert!(result.contains(
        &format!("\"X-Forwarded-Port\": \"{}\"", port),
    ));
}

// Tests that if the proxy cannot connect to upstream it returns a 502 response.
#[test]
fn test_upstream_down() {
    let port = 9092;
    let upstream_port = 9093;

    let _proxy = rustnish::start_server_background(port, upstream_port);

    // Make a request to the proxy and check the response.
    let url = ("http://127.0.0.1:".to_string() + &port.to_string())
        .parse()
        .unwrap();
    let response = common::client_get(url);

    assert_eq!(StatusCode::BadGateway, response.status());
    assert_eq!(
        Ok("Something went wrong, please try again later."),
        str::from_utf8(&response.body().concat2().wait().unwrap())
    );
}

// Tests that an invalid HTTP host header does not cause a panic.
#[test]
fn test_invalid_host() {
    let port = 9094;
    let upstream_port = 9095;

    let _proxy = rustnish::start_server_background(port, upstream_port);

    let url = ("http://127.0.0.1:".to_string() + &port.to_string())
        .parse()
        .unwrap();
    let mut request = Request::new(Method::Get, url);
    request.headers_mut().set(Host::new("$$$", None));

    let response = common::client_request(request);

    // The proxy just tries to forward that as is, but no one is listening.
    assert_eq!(StatusCode::BadGateway, response.status());
    assert_eq!(
        Ok("Something went wrong, please try again later."),
        str::from_utf8(&response.body().concat2().wait().unwrap())
    );
}

// Tests the error result if a port is already occupied on this host.
#[test]
fn test_port_occupied() {
    // Use the same port for upstream server and proxy, which will cause an
    // error.
    let port = 9096;

    let _dummy_server = common::start_dummy_server(port);
    let error_chain = rustnish::start_server_blocking(port, port).unwrap_err();
    assert_eq!(
        error_chain.description(),
        "The server thread stopped with an error"
    );
    let mut iter = error_chain.iter();
    let _first = iter.next();
    let second = iter.next().unwrap();
    assert_eq!(
        second.to_string(),
        "Failed to bind server to address 127.0.0.1:9096"
    );
    let third = iter.next().unwrap();
    // The exact error code is different on Linux and MacOS, so we test just for
    // the beginning of the error message.
    assert_eq!(&third.to_string()[..32], "Address already in use (os error");
}

// Tests that POST requests are also passed through.
#[test]
fn test_post_request() {
    let port = 9097;
    let upstream_port = 9098;

    let _post_server = common::start_dummy_server(upstream_port);

    // Start our reverse proxy which forwards to the post server.
    let _proxy = rustnish::start_server_background(port, upstream_port);

    // Make a request to the proxy and check if we get the correct result back.
    let url = ("http://127.0.0.1:".to_string() + &port.to_string())
        .parse()
        .unwrap();
    let response = common::client_post(url, "abc");

    let body = response.body().concat2().wait().unwrap();
    let result = str::from_utf8(&body).unwrap();

    assert_eq!(
        "Request { method: Post, uri: \"/\", version: Http11, remote_addr:",
        &result[..63]
    );

    // Check that an X-Forwarded-For header was added on the request.
    assert!(result.contains("\"X-Forwarded-For\": \"127.0.0.1\""));

    assert!(result.contains(
        &format!("\"X-Forwarded-Port\": \"{}\"", port),
    ));
}
