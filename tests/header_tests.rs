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

    let _dummy_server = common::start_dummy_server(upstream_port);
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
