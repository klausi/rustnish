// Copy of the example in Hyper except that we use port 9091.

#![deny(warnings)]
extern crate hyper;
extern crate futures;

use hyper::header::{ContentLength, ContentType};
use hyper::server::{Http, Response, const_service, service_fn};

static PHRASE: &'static [u8] = b"Hello World!";

fn main() {
    let addr = ([127, 0, 0, 1], 9091).into();

    let new_service = const_service(service_fn(|_| {
        Ok(Response::<hyper::Body>::new()
            .with_header(ContentLength(PHRASE.len() as u64))
            .with_header(ContentType::plaintext())
            .with_body(PHRASE))
    }));

    let server = Http::new()
        .sleep_on_errors(true)
        .bind(&addr, new_service)
        .unwrap();
    println!("Listening on http://{} with 1 thread.", server.local_addr().unwrap());
    server.run().unwrap();
}