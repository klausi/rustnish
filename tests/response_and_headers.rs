extern crate futures;
extern crate hyper;
extern crate rustnish;
extern crate tokio_core;

use hyper::{Method, StatusCode};
use hyper::{Body, Request};
use futures::{Future, Stream};
use std::str;
use hyper::header::{HOST, SERVER};
use common::echo_request;

mod common;

#[test]
fn pass_through() {
    let port = common::get_free_port();
    let upstream_port = common::get_free_port();

    // Start a dummy server that just echoes the request.
    let _dummy_server = common::start_dummy_server(upstream_port, echo_request);

    // Start our reverse proxy which forwards to the dummy server.
    let _proxy = rustnish::start_server_background(port, upstream_port);

    // Make a request to the proxy and check if we get the echo back.
    let url = ("http://127.0.0.1:".to_string() + &port.to_string())
        .parse()
        .unwrap();
    let response = common::client_get(url);

    assert_eq!(
        response.headers().get("Via").unwrap(),
        "1.1 rustnish-0.0.1"
    );

    assert_eq!(
        response
            .headers()
            .get(SERVER)
            .unwrap(),
        "rustnish"
    );

    let body = response.into_body().concat2().wait().unwrap();
    let result = str::from_utf8(&body).unwrap();

    // Check that the request method was GET.
    assert_eq!(
        "Request { method: Get, uri: \"/\", version: Http11, remote_addr:",
        &result[..62]
    );

    // Check that an X-Forwarded-For header was added on the request.
    assert!(result.contains("\"X-Forwarded-For\": \"127.0.0.1\""));

    assert!(result.contains(&format!("\"X-Forwarded-Port\": \"{}\"", port),));
}

// Tests that if the proxy cannot connect to upstream it returns a 502 response.
#[test]
fn upstream_down() {
    let port = common::get_free_port();
    let upstream_port = common::get_free_port();

    let _proxy = rustnish::start_server_background(port, upstream_port);

    // Make a request to the proxy and check the response.
    let url = ("http://127.0.0.1:".to_string() + &port.to_string())
        .parse()
        .unwrap();
    let response = common::client_get(url);

    assert_eq!(StatusCode::BAD_GATEWAY, response.status());
    assert_eq!(
        Ok("Something went wrong, please try again later."),
        str::from_utf8(&response.into_body().concat2().wait().unwrap())
    );
}

// Tests that an invalid HTTP host header does not cause a panic.
#[test]
fn invalid_host() {
    let port = common::get_free_port();
    let upstream_port = common::get_free_port();

    let _proxy = rustnish::start_server_background(port, upstream_port);

    let url = "http://127.0.0.1:".to_string() + &port.to_string();
    let mut request = Request::builder();
    request
        .uri(url)
        .header(HOST, "$$$");

    let response = common::client_request(request.body(Body::empty()).unwrap());

    // The proxy just tries to forward that as is, but no one is listening.
    assert_eq!(StatusCode::BAD_GATEWAY, response.status());
    assert_eq!(
        Ok("Something went wrong, please try again later."),
        str::from_utf8(&response.into_body().concat2().wait().unwrap())
    );
}

// Tests the error result if a port is already occupied on this host.
#[test]
fn port_occupied() {
    // Use the same port for upstream server and proxy, which will cause an
    // error.
    let port = common::get_free_port();

    let _dummy_server = common::start_dummy_server(port, echo_request);
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
        format!("Failed to bind server to address 127.0.0.1:{}", port)
    );
    let third = iter.next().unwrap();
    // The exact error code is different on Linux and MacOS, so we test just for
    // the beginning of the error message.
    assert_eq!(&third.to_string()[..32], "Address already in use (os error");
}

// Tests that POST requests are also passed through.
#[test]
fn post_request() {
    let port = common::get_free_port();
    let upstream_port = common::get_free_port();

    let _post_server = common::start_dummy_server(upstream_port, echo_request);

    // Start our reverse proxy which forwards to the post server.
    let _proxy = rustnish::start_server_background(port, upstream_port);

    // Make a request to the proxy and check if we get the correct result back.
    let url = ("http://127.0.0.1:".to_string() + &port.to_string())
        .parse()
        .unwrap();
    let response = common::client_post(url, "abc");

    let body = response.into_body().concat2().wait().unwrap();
    let result = str::from_utf8(&body).unwrap();

    assert_eq!(
        "Request { method: Post, uri: \"/\", version: Http11, remote_addr:",
        &result[..63]
    );

    // Check that an X-Forwarded-For header was added on the request.
    assert!(result.contains("\"X-Forwarded-For\": \"127.0.0.1\""));

    assert!(result.contains(&format!("\"X-Forwarded-Port\": \"{}\"", port),));
}

// Tests that if an X-Forwarded-For header already exists on the request then
// the proxy adds another value.
#[test]
fn x_forwarded_for_added() {
    let port = common::get_free_port();
    let upstream_port = common::get_free_port();

    let _dummy_server = common::start_dummy_server(upstream_port, echo_request);
    let _proxy = rustnish::start_server_background(port, upstream_port);

    let request = Request::builder()
        .uri("http://127.0.0.1:".to_string() + &port.to_string())
        .header("X-Forwarded-For", "1.2.3.4")
        .body(Body::empty())
        .unwrap();

    let response = common::client_request(request);

    let body = response.into_body().concat2().wait().unwrap();
    let result = str::from_utf8(&body).unwrap();

    // Check that the request method was GET.
    assert_eq!(
        "Request { method: Get, uri: \"/\", version: Http11, remote_addr:",
        &result[..62]
    );

    // Check that an X-Forwarded-For header was added on the request.
    assert!(result.contains("\"X-Forwarded-For\": \"1.2.3.4, 127.0.0.1\"",));
}

// Tests that if a Via header already exists on the request then the proxy adds
// another value.
#[test]
fn via_header_added() {
    let port = common::get_free_port();
    let upstream_port = common::get_free_port();

    let _dummy_server = common::start_dummy_server(upstream_port, |request| {
        let mut response = echo_request(request);
        {
        let headers = response.headers_mut();
        headers.append("Via", "1.1 test".parse().unwrap());
        }
        response
    });
    let _proxy = rustnish::start_server_background(port, upstream_port);

    let url = ("http://127.0.0.1:".to_string() + &port.to_string())
        .parse()
        .unwrap();
    let response = common::client_get(url);

    let via_headers = response.headers().get("Via").unwrap();
    assert_eq!(via_headers, "1.1 test, 1.1 rustnish-0.0.1");
}

// Tests that if a Server HTTP header is present from upstream it is not
// overwritten.
#[test]
fn server_header_present() {
    let port = common::get_free_port();
    let upstream_port = common::get_free_port();

    let _dummy_server = common::start_dummy_server(upstream_port, |request| {
        let mut response = echo_request(request);
        {
        response.headers_mut().insert(SERVER, "dummy-server".parse().unwrap());
        }
        response
    });
    let _proxy = rustnish::start_server_background(port, upstream_port);

    let url = ("http://127.0.0.1:".to_string() + &port.to_string())
        .parse()
        .unwrap();
    let response = common::client_get(url);

    let server_header = response
        .headers()
        .get(SERVER)
        .unwrap();
    assert_eq!(server_header, "dummy-server");
}

// Tests that URL query parameters are passed through.
#[test]
fn query_parameters() {
    let port = common::get_free_port();
    let upstream_port = common::get_free_port();

    let _post_server = common::start_dummy_server(upstream_port, echo_request);

    let _proxy = rustnish::start_server_background(port, upstream_port);

    // Make a request to the proxy and check if we get the correct result back.
    let url = ("http://127.0.0.1:".to_string() + &port.to_string() + "/test?key=value")
        .parse()
        .unwrap();
    let response = common::client_get(url);

    let body = response.into_body().concat2().wait().unwrap();
    let result = str::from_utf8(&body).unwrap();

    assert_eq!(
        "Request { method: Get, uri: \"/test?key=value\", version: Http11, remote_addr:",
        &result[..76]
    );
}
