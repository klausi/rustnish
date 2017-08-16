---
title: "Replacing unwrap() and avoiding panics in Rust"
layout: post
---

[```unwrap()```](https://doc.rust-lang.org/std/result/enum.Result.html#method.unwrap) is a useful tool in Rust but is also bad practice in production code that should not abort with unpredictable panics.

Therefore my goal 4 for Rustnish is full integration tests with no panics allowed:

> Expand the integration tests to confirm that the reverse proxy is working as
> expected. Add tests with broken HTTP requests to cover error handling of the
> reverse proxy. All ```unwrap()``` calls in none test code should be removed and
> covered by proper error handling.

You can find all the code in [the goal-04 branch on
Github](https://github.com/klausi/rustnish/tree/goal-04).


## The case for unwrap() in tests

Before we look at solutions how to replace ```unwrap()``` I would like to emphasize that it absolutely has its place in automated test cases. In a test case we do not fear panics triggered by unwrap() because the test runner will catch them and just mark the test run as failed. That means we can be super lazy when writing test code and use ```unwrap()``` all the time. For example using a Hyper client in tests:

```rust
// Since it so complicated to make a client request with a Tokio core we have
// this helper function.
fn client_get(url: Uri) -> Response {
    let mut core = Core::new().unwrap();
    let client = Client::new(&core.handle());
    let work = client.get(url).and_then(|response| Ok(response));
    core.run(work).unwrap()
}
```

This helper function issues a GET request and returns a Response struct - we don't care if something goes wrong (e.g. network problems or the server does not exist). In case of an error the function will panic, we will see a panic backtrace in the test output and the test is marked as failed. Otherwise we can directly work with the returned Response omitting any error handling and keeping the test code minimal.


## User input errors

You might have introduced ```unwrap()``` calls during quickly prototyping your application, but the underlying error case should be communicated back to the application user. For example in my reverse proxy the user provided Host header is used:

```rust
let upstream_uri = ("http://".to_string() + host + ":" +
    &self.upstream_port.to_string() + request_uri.path())
    .parse()
    .unwrap();
```

If the user supplies a bad Host header then this will cause a panic on the server and the user will get no response. The solution is to handle the error and report back a response to the user:

```rust
let upstream_string_uri = "http://".to_string() + &host + ":" +
    &self.upstream_port.to_string() + request_uri.path();
let upstream_uri = match upstream_string_uri.parse() {
    Ok(u) => u,
    _ => {
        return Either::A(futures::future::ok(
            Response::new()
                .with_status(StatusCode::BadRequest)
                .with_body("Invalid host header in request"),
        ));
    }
};
```

This is certainly application specific how you process the error, but a ```match()``` expression is likely to be useful.


## Error chains

If you are dealing with more severe error conditions then it makes sense to bubble them up with the [error-chain](https://crates.io/crates/error-chain) crate. Instead of crashing your application with a panic you can pass up error state to the caller of your code and they can inspect it. [The error-chain documentation](https://docs.rs/error-chain) has some further explanation and reasoning behind it.

Let's consider an example: if a caller wants to start the reverse proxy on a port that is already occupied then that will cause an error we want to bubble up instead of panicking:

```rust
let thread = thread::Builder::new().spawn(move || {
    // ... some not so relevant code here.
    let listener = TcpListener::bind(&addr, &handle).unwrap();
    // ... some more code here.
})
.unwrap();
```

We need to follow a couple of steps for the first error chain setup:

1. Add ```error-chain = "*"``` to Cargo.toml
2. Add the error_chain!() macro to your code:

   ```rust
   #[macro_use]
   extern crate error_chain;
   mod errors {
       // Create the Error, ErrorKind, ResultExt, and Result types
       error_chain!{}
   }
   ```
3. Change the return type of your function to ```Result``` so that it can return errors. Note: a convention in Rust is that Results must be used instead of directly returning error types (even if there is an empty ```Ok``` type.) See the [result module docs](https://doc.rust-lang.org/std/result/index.html) for an explanation.
4. Use ```chain_err()``` to pass along errors and ```bail!()``` to create new errors.

That way we can convert our ```unwrap()``` calls into this:

```rust
let thread = thread::Builder::new()..spawn(move || -> Result<()> {
    // ... some not so relevant code here.
    let listener = TcpListener::bind(&address, &handle)
        .chain_err(|| format!("Failed to bind server to address {}", address))?;
    // ... some more code here.
})
.chain_err(|| "Spawning server thread failed")?;
```

We introduced the empty ```Result<()>``` as return type and use [the ```?``` operator](https://doc.rust-lang.org/book/second-edition/ch09-02-recoverable-errors-with-result.html#a-shortcut-for-propagating-errors-) to early return errors.

By adding a new error to the error chain we give meaning and context where and how the error occurred - which makes it easier to get to the problem when diagnosing errors. The output of such an error chain looks like this:

```
Error: The server thread stopped with an error
Caused by: Failed to bind server to address 127.0.0.1:3306
Caused by: Address already in use (os error 98)
```

Now this is much more useful than just the last error - with the help of an error chain we know which port is already in use.

The consumer that gets an error chain returned can iterate through it, inspect and examine it. For example in test code:

```rust
let error_chain = rustnish::start_server_blocking(port, upstream_port)
    .unwrap_err();
assert_eq!(
    error_chain.description(),
    "The server thread stopped with an error"
);
let mut iter = error_chain.iter();
let _first = iter.next();
let second = iter.next().unwrap();
assert_eq!(
    second.to_string(),
    "Failed to bind server to address 127.0.0.1:3306"
);
let third = iter.next().unwrap();
assert_eq!(third.to_string(), "Address already in use (os error 98)");
```


## Conclusion

```unwrap()``` is your friend in test code where panics are allowed. Error chains are a powerful concept of handling errors by providing better context. They are comparable to exception handling in languages such as PHP and Java.
