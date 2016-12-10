extern crate hyper;

use hyper::Client;
use std::io::Read;

fn main() {
    let client = Client::new();
    // Why does the response have to be mutable here? We never need to modify it, so we should be
    // able to remove "mut"?
    let mut response = client.get("http://drupal-8.localhost/").send().unwrap();
    // Print out all the headers first.
    for header in response.headers.iter() {
        println!("{}", header);
    }
    // Now the body. This is ugly, why do I have to create an intermediary string variable? I want
    // to push the response directly to stdout.
    let mut body = String::new();
    response.read_to_string(&mut body).unwrap();
    print!("{}", body);
}
