use futures::Future;
use hyper::service::service_fn_ok;
use hyper::{Body, Request, Response};
use hyper::{Client, Server, Uri};
use std::str;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::runtime::Runtime;

// Return the received request in the response body for testing purposes.
pub fn echo_request(request: Request<Body>) -> Response<Body> {
    Response::builder()
        .body(Body::from(format!("{:?}", request)))
        .unwrap()
}

// Starts a dummy server in a separate thread.
pub fn start_dummy_server(
    port: u16,
    response_function: fn(Request<Body>) -> Response<Body>,
) -> Runtime {
    let address = "127.0.0.1:".to_owned() + &port.to_string();
    let addr = address.parse().unwrap();

    let new_svc = move || service_fn_ok(response_function);

    let server = Server::bind(&addr).serve(new_svc).map_err(|_| ());

    let mut runtime = Runtime::new().unwrap();
    runtime.spawn(server);
    runtime
}

// Since it so complicated to make a client request with a Hyper runtime we have
// this helper function.
#[allow(dead_code)]
pub fn client_get(url: Uri) -> Response<Body> {
    let client = Client::new();
    let work = client.get(url).and_then(Ok);

    let mut rt = Runtime::new().unwrap();
    rt.block_on(work).unwrap()
}

#[allow(dead_code)]
pub fn client_post(url: Uri, body: &'static str) -> Response<Body> {
    let client = Client::new();

    let req = Request::builder()
        .method("POST")
        .uri(url)
        .body(Body::from(body))
        .unwrap();

    let work = client.request(req).and_then(Ok);
    let mut rt = Runtime::new().unwrap();
    rt.block_on(work).unwrap()
}

// Since it so complicated to make a client request with a Tokio runtime we have
// this helper function.
#[allow(dead_code)]
pub fn client_request(request: Request<Body>) -> Response<Body> {
    let client = Client::new();
    let work = client.request(request).and_then(Ok);
    let mut rt = Runtime::new().unwrap();
    rt.block_on(work).unwrap()
}

// Returns a local port number that has not been used yet in parallel test
// threads.
pub fn get_free_port() -> u16 {
    static PORT_NR: AtomicUsize = AtomicUsize::new(0);

    PORT_NR.fetch_add(1, Ordering::SeqCst) as u16 + 9090
}
