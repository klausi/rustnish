extern crate hyper;
extern crate futures;
extern crate rustnish;
extern crate tokio_core;

use hyper::Method;
use hyper::server::Request;
use futures::{Future, Stream};
use std::str;

mod common;

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
    request.headers_mut().set_raw(
        "X-Forwarded-For",
        "1.2.3.4".to_string(),
    );

    let response = common::client_request(request);

    let body = response.body().concat2().wait().unwrap();
    let result = str::from_utf8(&body).unwrap();

    // Check that the request method was GET.
    assert_eq!(
        "Request { method: Get, uri: \"/\", version: Http11, remote_addr:",
        &result[..62]
    );

    // Check that an X-Forwarded-For header was added on the request.
    assert!(result.contains(
        "\"X-Forwarded-For\": \"1.2.3.4, 127.0.0.1\"",
    ));
}

// Tests that if a Via header already exists on the request then the proxy adds
// another value.
#[test]
fn test_via_header_added() {
    let port = 9101;
    let upstream_port = 9102;

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
    let port = 9103;
    let upstream_port = 9104;

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
