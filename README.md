# Rustnish
Experimental project to learn Rust. A reverse proxy.

https://klausi.github.io/rustnish/

## Goal 1: Just pipe HTTP requests through
Completed: yes

A webserver like Apache is listening on port 80. Write a reverse proxy service
that does nothing but forwarding HTTP requests to port 80. The service must
listen on port 9090. The service must not modify the HTTP response in any way.

## Goal 2: One integration test
Completed: yes

Write an integration test that confirms that the reverse proxy is working as
expected. The test should issue a real HTTP request and check that passing
through upstream responses works. Refactor the code to accept arbitrary port
numbers so that the tests can simulate a real backend without requiring root
access to bind on port 80.

## Goal 3: Convert Hyper server to Tokio
Completed: yes

A new version of the [Hyper library](https://hyper.rs/) has been released which
is based on the [Tokio library](https://tokio.rs/). Convert the existing code to
use that new version and provide one integration test case.
