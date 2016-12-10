extern crate hyper;

use hyper::Client;
use hyper::header::Connection;
use std::io::Read;

fn main() {
    let client = Client::new();
    // Why does the response have to be mutable here? We never need to modify it, so we should be
    // able to remove "mut"?
    let mut response = client.get("http://drupal-8.localhost/").
        header(Connection::close()).send().unwrap();
    let mut body = String::new();
    response.read_to_string(&mut body).unwrap();
    print!("{}", body);
}
