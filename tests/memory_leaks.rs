extern crate futures;
extern crate hyper;
extern crate procinfo;
extern crate rustnish;
extern crate tokio_core;

use hyper::{Client, Uri};
use futures::Future;
use futures::future::join_all;
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

    // Get the resident non-swapped memory of this process that actually takes
    // up space in RAM.
    let memory_before = procinfo::pid::statm_self().unwrap().resident;

    // Perform 10,000 requests in batches of 100. Otherwise we get "Too many open
    // files" errors because of opening too many ports.
    for _i in 1..100 {
        let mut requests = Vec::new();

        for _j in 0..100 {
            requests.push(client.get(url.clone()).and_then(Ok));
        }
        let work = join_all(requests);
        core.run(work).unwrap();
    }

    let memory_after = procinfo::pid::statm_self().unwrap().resident;
    // Allow memory to grow by 2MB, but not more.
    assert!(
        memory_after < memory_before + 2048,
        "Memory usage at server start is {}KB, memory usage after 10,000 requests is {}KB",
        memory_before,
        memory_after
    );
}
