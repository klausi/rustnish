extern crate futures;
extern crate hyper;
extern crate rustnish;

use common::echo_request;
use futures::Future;
use hyper::header::CACHE_CONTROL;
use hyper::StatusCode;
use hyper::Uri;

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
    // upsatream server is down.
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
