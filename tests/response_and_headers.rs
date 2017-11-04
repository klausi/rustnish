extern crate futures;
extern crate hyper;
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
    let port = common::get_free_port();
    let upstream_port = common::get_free_port();

    // Start a dummy server on port 9091 that just echoes the request.
    let _dummy_server = common::start_dummy_server(upstream_port, |r| r);

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

    assert_eq!(
        response
            .headers()
            .get::<hyper::header::Server>()
            .unwrap()
            .to_string(),
        "rustnish"
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

    assert!(result.contains(&format!("\"X-Forwarded-Port\": \"{}\"", port),));
}

// Tests that if the proxy cannot connect to upstream it returns a 502 response.
#[test]
fn test_upstream_down() {
    let port = common::get_free_port();
    let upstream_port = common::get_free_port();

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
    let port = common::get_free_port();
    let upstream_port = common::get_free_port();

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
    let port = common::get_free_port();

    let _dummy_server = common::start_dummy_server(port, |r| r);
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
fn test_post_request() {
    let port = common::get_free_port();
    let upstream_port = common::get_free_port();

    let _post_server = common::start_dummy_server(upstream_port, |r| r);

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

    assert!(result.contains(&format!("\"X-Forwarded-Port\": \"{}\"", port),));
}

// Tests that if an X-Forwarded-For header already exists on the request then
// the proxy adds another value.
#[test]
fn test_x_forwarded_for_added() {
    let port = 9099;
    let upstream_port = 9100;

    let _dummy_server = common::start_dummy_server(upstream_port, |r| r);
    let _proxy = rustnish::start_server_background(port, upstream_port);

    let url = ("http://127.0.0.1:".to_string() + &port.to_string())
        .parse()
        .unwrap();
    let mut request = Request::new(Method::Get, url);
    request
        .headers_mut()
        .set_raw("X-Forwarded-For", "1.2.3.4".to_string());

    let response = common::client_request(request);

    let body = response.body().concat2().wait().unwrap();
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
fn test_via_header_added() {
    let port = common::get_free_port();
    let upstream_port = common::get_free_port();

    let _dummy_server = common::start_dummy_server(upstream_port, |upstream_response| {
        let mut headers = upstream_response.headers().clone();
        headers.append_raw("Via", "1.1 test");
        upstream_response.with_headers(headers)
    });
    let _proxy = rustnish::start_server_background(port, upstream_port);

    let url = ("http://127.0.0.1:".to_string() + &port.to_string())
        .parse()
        .unwrap();
    let response = common::client_get(url);

    let mut via_headers = response.headers().get_raw("Via").unwrap().iter();
    let first = str::from_utf8(via_headers.next().unwrap()).unwrap();
    assert_eq!(first, "1.1 test");
    let second = str::from_utf8(via_headers.next().unwrap()).unwrap();
    assert_eq!(second, "1.1 rustnish-0.0.1");
}

// Tests that if a Server HTTP header is present from upstream it is not
// overwritten.
#[test]
fn test_server_header_present() {
    let port = common::get_free_port();
    let upstream_port = common::get_free_port();

    let _dummy_server = common::start_dummy_server(upstream_port, |upstream_response| {
        upstream_response.with_header(hyper::header::Server::new("dummy-server"))
    });
    let _proxy = rustnish::start_server_background(port, upstream_port);

    let url = ("http://127.0.0.1:".to_string() + &port.to_string())
        .parse()
        .unwrap();
    let response = common::client_get(url);

    let server_header = response
        .headers()
        .get::<hyper::header::Server>()
        .unwrap()
        .to_string();
    assert_eq!(server_header, "dummy-server");
}
