extern crate hyper;
extern crate rustnish;

use hyper::server::{Server, Request, Response};
use hyper::Client;
use std::io::Read;

#[test]
fn test_pass_through() {
    let port = 9090;
    let upstream_port = 9091;

    // Start a dummy server on port 9091 that just returns a hello.
    let mut dummy_server = Server::http("127.0.0.1:".to_string() + &upstream_port.to_string())
        .unwrap()
        .handle(|_: Request, response: Response| { response.send(b"hello").unwrap(); })
        .unwrap();

    // Start our reverse proxy which forwards to the dummy server.
    let mut listening = rustnish::start_server(port, upstream_port);

    // Make a request to the proxy and check if we get the hello back.
    let client = Client::new();

    let url = ("http://127.0.0.1:".to_string() + &port.to_string())
        .into_url()
        .unwrap();
    let request_builder = client.get(url);
    let mut upstream_response = request_builder.send().unwrap();

    // Why do I have to prepare a string variable beforehand? Why is there no
    // method on the Read trait that just produces the string for me?
    let mut body = String::new();
    let _size = upstream_response.read_to_string(&mut body).unwrap();

    // Before we make assertions make sure to stop our running servers
    // (teardown). Otherwise the test will hang when it fails because the
    // assertion stops the test function execution and the servers are never
    // stopped. Yes, some kind of test framework for testing servers would be
    // really useful here.
    let _guard = listening.close();
    let _dummy_guard = dummy_server.close();

    // Why does this work when the 2 types are different? The first is &str, the
    // second is String. Shouldn't assert_eq() also check the types of the stuff
    // I'm comparing? Or is this intentional magic?
    assert_eq!("hello", body);
}
