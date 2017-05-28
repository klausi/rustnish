---
title: "Writing integration tests in Rust"
layout: post
---

In my first post I wrote a quite fragile, minimally working prototype that uses
many ```unwrap()``` calls thereby raising lots of panics during execution.
Implementing and verifying proper error handling requires testing. I don't want
to do unit testing yet because that would require research about complicated
mocking techniques and dependency injection in Rust. Instead, I would like to
do integration testing of the whole application to prove that the end result is
working as expected.

Here is the requirement for goal 2 of Rustnish:

> Write integration tests that confirm that the reverse proxy is working as
> expected. Add tests with broken HTTP requests to cover error handling of the
> reverse proxy. Refactor the code to accept arbitrary port numbers so that the
> tests can simulate a real backend without requiring root access to bind on port
> 80.

## Integration test setup

The [Rust book has a section about
testing](https://doc.rust-lang.org/book/testing.html) which describes that you
put integration tests into a "tests" folder in your project. We create a file
tests/integration_tests.rs with the following content:

```rust
extern crate rustnish;

#[test]
fn test_pass_through() {
    let port = 9090;
    let upstream_port = 9091;
    let mut listening = rustnish::start_server(port, upstream_port);
}
```

Because this is an integration test we have to treat our own application
"rustnish" as external crate that needs to be included here. The ```#[test]```
attribute tells the test runner (cargo) that this function should be executed
as test. Since the start_server() function does not exist yet this test should
fail because it will not even compile.

The tests can be run with cargo:

```
$ cargo test
   Compiling rustnish v0.0.1 (file:///home/klausi/workspace/rustnish)
error[E0425]: cannot find function `start_server` in module `rustnish`
  --> tests/integration_tests.rs:21:35
   |
21 |     let mut listening = rustnish::start_server(port, upstream_port);
   |                                   ^^^^^^^^^^^^ not found in `rustnish`

error: aborting due to previous error

error: Could not compile `rustnish`.
```

In order to integration test your Rust application **you need to split it up
into a main.rs file and a lib.rs file**.

main.rs is a thin wrapper that just launches the reverse proxy server:

```rust
extern crate rustnish;

fn main() {
    let port: u16 = 9090;
    let upstream_port: u16 = 80;
    rustnish::start_server(port, upstream_port);
}
```

Our own code is now the rustnish library crate that we need to include here.

In lib.rs we create an empty dummy start_server() function:

```rust
pub fn start_server(port: u16, upstream_port: u16) {}
```

The function needs to be marked as public (```pub```) so that it is visible to
consumers of our crate. Running the tests again:

```
$ cargo test
   Compiling rustnish v0.0.1 (file:///home/klausi/workspace/rustnish)
    Finished dev [unoptimized + debuginfo] target(s) in 0.60 secs
     Running target/debug/deps/rustnish-64c4558d64f77466

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured

     Running target/debug/deps/rustnish-a8d8bad65e5d7764

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured

     Running target/debug/deps/integration_tests-66e61bd575a35301

running 1 test
test test_pass_through ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured

   Doc-tests rustnish

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured
```

All green, tests are passing the first time! The output is a bit long and
confusing and consists of 4 groups:

* 2 Unit tests directly written in the src files (lib.rs and main.rs): we have
  none yet.
* Integration tests: everything in the tests folder (the one test we just wrote
  is run here).
* Doc tests for example code in documentation: we have none yet.

That way the cargo test runner lets you know passive aggressively that you
should write all these kind of tests :-)

Of course we are not testing anything useful yet - let's expand the test case.

## Integration tests for a Hyper server

The main idea for our integration test is this:

1. Start a dummy backend server that will mock a real web server (like Apache
   that we proxy to).
2. Start our reverse proxy configured to forward requests to the dummy backend
   server.
3. Make a request to our proxy and assert that we get the response as mocked by
   the dummy backend server.

That way we can make sure that the response is passed through correctly and our
reverse proxy works. You can find the whole [test source code on
Github](https://github.com/klausi/rustnish/blob/goal-02/tests/integration_tests.rs),
let's examine the parts of the test:

```rust
#[test]
fn test_pass_through() {
    let port = 9090;
    let upstream_port = 9091;

    let mut dummy_server = Server::http("127.0.0.1:".to_string() + &upstream_port.to_string())
        .unwrap()
        .handle(|_: Request, response: Response| { response.send(b"hello").unwrap(); })
        .unwrap();
```

This starts the dummy server that simply responds with a "hello" response to
any request it receives. The actual request handling is done in a [Rust
closure](https://doc.rust-lang.org/book/closures.html) (anonymous function)
which is expressed by the two pipes ```|```. Easy, expressive and powerful -
thanks Hyper!

```rust
let mut listening = rustnish::start_server(port, upstream_port);

let client = Client::new();

let url = ("http://127.0.0.1:".to_string() + &port.to_string())
    .into_url()
    .unwrap();
let request_builder = client.get(url);
let mut upstream_response = request_builder.send().unwrap();
```

Next we start our reverse proxy, configured with the port it should listen on
and the upstream port it should forward requests to. Then we make a request to
the reverse proxy and read the response. Again, doing that with the HTTP client
API Hyper provides is fairly easy.

```rust
let mut body = String::new();
let _size = upstream_response.read_to_string(&mut body).unwrap();

let _guard = listening.close();
let _dummy_guard = dummy_server.close();

assert_eq!("hello", body);
```

The last part of the test is to make sure that the response received matches
what we expect. For reading the response body we need to make room for it by
allocating a String variable. This is a bit counter-intuitive here - why is
there no method on the stream Read trait that makes that String for me? Maybe
the philosophy is that I as the consumer of the API should be aware of the
memory impact reading that stream has? It looks ugly that I have to define a
mutable variable ```body```, but I never really mutate it. I just fill it once.

Before we can do the assertion to check if the response received is correct we
need to shut down the two servers we started. This is important because
otherwise the test run could just hang and not terminate. If the assertion
fails then the execution will panic in the test function and shutting down the
servers would never happen. That's why we stop the servers first and make our
assertion at the very end.

After refactoring the application code this test is passing :-)

At this point I realize that an integration testing framework would be useful
that has clear setup and teardown phases for test runs. That would help
structuring this test by moving the test server shutdown to a place that is
always called regardless if the test is passing or not. A quick web search
points to the [Stainless](https://crates.io/crates/stainless) crate which
probably helps with that.

## Conclusion

The basic test infrastructure that Rust core ships with is great and let's you
quickly get started with Testing. Integration tests are application dependent
and many Rust libraries write their own helper macros to ease test case
development. As mentioned there are libraries like
[Stainless](https://crates.io/crates/stainless) that can ease handling of
initialization and shutdown code for tests.
