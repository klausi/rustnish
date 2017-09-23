extern crate hyper;
extern crate futures;
extern crate rustnish;
extern crate tokio_core;

use hyper::{Client, Method, Uri};
use hyper::server::Request;
use futures::{Future, Stream};
use futures::future::{AndThen, join_all};
use std::str;
use tokio_core::reactor::Core;

mod common;

// Tests that process memory does not excessively rise after 1000 requests.
#[test]
fn test_memory_after_1000_requests() {
    let port = common::get_free_port();
    let upstream_port = common::get_free_port();

    let _dummy_server = common::start_dummy_server(upstream_port, |r| r);
    let _proxy = rustnish::start_server_background(port, upstream_port);

    let mut core = Core::new().unwrap();
    let client = Client::new(&core.handle());

    let url: Uri = ("http://127.0.0.1:".to_string() + &port.to_string())
        .parse()
        .unwrap();
    let mut requests = Vec::new();

    for i in 0..9 {
        requests.push(client.get(url.clone()).and_then(Ok));
    }
    let work = join_all(requests);
    core.run(work).unwrap();
}
