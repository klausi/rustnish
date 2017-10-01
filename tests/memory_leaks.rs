extern crate futures;
extern crate hyper;
extern crate procinfo;
extern crate rustnish;
extern crate tokio_core;

use hyper::{Client, Method, Request, Uri};
use futures::future::{join_all, loop_fn, Future, Loop};
use tokio_core::reactor::Core;

mod common;

// Tests that process memory does not excessively rise after 1000 HTTP 1.0
// requests.
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

    let nr_requests = 20000;
    let concurrency = 4;

    let mut parallel = Vec::new();
    for _i in 0..concurrency {
        let requests_til_done = loop_fn(0, |counter| {
            let mut request = Request::new(Method::Get, url.clone());
            request.set_version(hyper::HttpVersion::Http10);
            client
                .request(request)
                .then(move |_| -> Result<_, hyper::Error> {
                    if counter < (nr_requests / concurrency) {
                        Ok(Loop::Continue(counter + 1))
                    } else {
                        Ok(Loop::Break(counter))
                    }
                })
        });
        parallel.push(requests_til_done);
    }

    let work = join_all(parallel);
    core.run(work).unwrap();

    let memory_after = procinfo::pid::statm_self().unwrap().resident;
    // Allow memory to grow by 2MB, but not more.
    assert!(
        memory_after < memory_before + 2048,
        "Memory usage at server start is {}KB, memory usage after {} requests is {}KB",
        memory_before,
        nr_requests,
        memory_after
    );
}
