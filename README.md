# Rustnish

[![Build Status](https://travis-ci.org/klausi/rustnish.svg?branch=goal-05)](https://travis-ci.org/klausi/rustnish)

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

## Goal 4: Full integration tests
Completed: yes

Expand the integration tests to confirm that the reverse proxy is working as
expected. Add tests with broken HTTP requests to cover error handling of the
reverse proxy. All ```unwrap()``` calls in none test code should be removed and
covered by proper error handling.

## Goal 5: Continuous integration testing with Travis CI
Completed: yes

Enable [Travis CI](http://travis-ci.org/) so that the automated tests are run
after every Git push to the Rustnish repository. Enable
[Clippy](https://github.com/rust-lang-nursery/rust-clippy) that also checks for
Rust best practices.

## Goal 6: Add HTTP headers to requests/responses
Completed: yes

Add the following HTTP headers to requests/responses passed through Rustnish:

Headers for requests forwarded upstream:
* `X-Forwarded-For`: the originating IP address of the client connecting to the
  proxy. Append IP address if already set.
* `X-Forwarded-Port`: the originating port of the HTTP request (example: 443).
  Append port if already set.

Headers for responses returned downstream:
* `Via`: a code name for the Rustnish proxy, with the value rustnish-0.0.1.
  Append if already set.
* `Server`: will be added to the response ("rustnish") if the upstream server
  didn't add it first.

## Goal 7: Add integration test that watches for memory leaks
Completed: yes

Add an integration test that ensures that the proxy server is not leaking memory
(growing RAM usage without shrinking again). Use /proc information to compare
memory usage of the current process before and after the test.
