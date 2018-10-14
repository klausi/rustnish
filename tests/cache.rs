extern crate futures;
extern crate hyper;
extern crate rustnish;

use common::echo_request;
use futures::Future;
use hyper::header::{CACHE_CONTROL, COOKIE};
use hyper::Uri;
use hyper::{Body, Request, StatusCode};
use std::thread;
use std::time::Duration;

mod common;

// Test that a GET request is cached and works even if the upstream source is
// down.
#[test]
fn upstream_down_cache() {
    let port = common::get_free_port();
    let upstream_port = common::get_free_port();

    let upstream_server = common::start_dummy_server(upstream_port, |request| {
        let mut response = echo_request(request);
        {
            let headers = response.headers_mut();
            headers.append(CACHE_CONTROL, "public,max-age=1800".parse().unwrap());
        }
        response
    });
    let _proxy = rustnish::start_server_background(port, upstream_port);

    let url: Uri = ("http://127.0.0.1:".to_string() + &port.to_string())
        .parse()
        .unwrap();
    // This request should populate the cache.
    common::client_get(url.clone());

    upstream_server.shutdown_now().wait().unwrap();

    // We should still get a valid cached response.
    let response2 = common::client_get(url);
    assert_eq!(response2.status(), StatusCode::OK);

    // Any other path is not cached and should return a 502 because the
    // upstream server is down.
    let test_url: Uri = ("http://127.0.0.1:".to_string() + &port.to_string() + "/test")
        .parse()
        .unwrap();
    let response3 = common::client_get(test_url);
    assert_eq!(response3.status(), StatusCode::BAD_GATEWAY);
}

// If a response does not have a max age header then it must not be cached.
#[test]
fn no_max_age_means_uncachable() {
    let port = common::get_free_port();
    let upstream_port = common::get_free_port();

    let upstream_server = common::start_dummy_server(upstream_port, echo_request);
    let _proxy = rustnish::start_server_background(port, upstream_port);

    let url: Uri = ("http://127.0.0.1:".to_string() + &port.to_string())
        .parse()
        .unwrap();
    // This request should not populate the cache.
    common::client_get(url.clone());

    upstream_server.shutdown_now().wait().unwrap();

    // We must not get a cached response.
    let response2 = common::client_get(url);
    assert_eq!(response2.status(), StatusCode::BAD_GATEWAY);
}

// A response must not be cached longer than the max-age cache-control headers
// says.
#[test]
fn max_age_expired() {
    let port = common::get_free_port();
    let upstream_port = common::get_free_port();

    let upstream_server = common::start_dummy_server(upstream_port, |request| {
        let mut response = echo_request(request);
        {
            let headers = response.headers_mut();
            headers.append(CACHE_CONTROL, "public,max-age=1".parse().unwrap());
        }
        response
    });
    let _proxy = rustnish::start_server_background(port, upstream_port);

    let url: Uri = ("http://127.0.0.1:".to_string() + &port.to_string())
        .parse()
        .unwrap();
    // This request should populate the cache.
    common::client_get(url.clone());

    upstream_server.shutdown_now().wait().unwrap();

    // Wait 1 second, then the cache must have expired this response.
    thread::sleep(Duration::from_secs(1));

    // We must not get a cached response.
    let response2 = common::client_get(url);
    assert_eq!(response2.status(), StatusCode::BAD_GATEWAY);
}

// If a request contains a session cookie then it should bypass the cache.
#[test]
fn session_cookie_bypass() {
    let port = common::get_free_port();
    let upstream_port = common::get_free_port();

    let upstream_server = common::start_dummy_server(upstream_port, |request| {
        let mut response = echo_request(request);
        {
            let headers = response.headers_mut();
            headers.append(CACHE_CONTROL, "public,max-age=1800".parse().unwrap());
        }
        response
    });
    let _proxy = rustnish::start_server_background(port, upstream_port);

    let url: Uri = ("http://127.0.0.1:".to_string() + &port.to_string())
        .parse()
        .unwrap();
    // This request should populate the cache.
    common::client_get(url.clone());

    upstream_server.shutdown_now().wait().unwrap();

    // We must not get a cached response when we set a session cookie.
    let mut request = Request::builder();
    request.uri(url).header(COOKIE, "SESS1234567=xyz");

    let response2 = common::client_request(request.body(Body::empty()).unwrap());
    assert_eq!(response2.status(), StatusCode::BAD_GATEWAY);
}
