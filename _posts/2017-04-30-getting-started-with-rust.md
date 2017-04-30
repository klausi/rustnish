---
title: "Getting started with Rust"
layout: post
---

[Rust](https://www.rust-lang.org) is still very alien to me and I want to write
a bit of code in it to get a feeling of the concepts and restrictions it
enforces.

## The use case

I worked through Rust examples but they always feel theoretical and artificial.
To get dirty with something real I thought of playing with something concrete.
Being a big fan of [Varnish](https://varnish-cache.org/) (a reverse proxy that
sits in front of your web application and caches HTTP requests) - that sounds
interesting to emulate in Rust? Disclaimer: I have never looked at the Varnish
source code and I will probably make very obvious mistakes.

Goals:
* write something like Varnish that caches HTTP GET requests.
* use as a lot of already existing work (Rust HTTP libraries) to avoid writing
too much code.
* iterate in smaller goals to quickly get something working.

With that in mind I have set out the requirement for the first step:

> A webserver like Apache is listening on port 80. Write a reverse proxy
> service that does nothing but forwarding HTTP requests to port 80. The service
> must listen on port 9090. The service must not modify the HTTP response in any
> way.

## Installing Rust

I'm using Ubuntu as operating system and everything was very straight forward
by [following the installation
instructions](https://www.rust-lang.org/en-US/install.html):

```
curl https://sh.rustup.rs -sSf | sh
```

That single script installs all rust command line utilities you need into your
home directory thereby not messing up your global Ubuntu system (yay!). It
gives you the following:

* ```rustup```: tool chain installer that installs/updates all the Rust things
(run ```rustup update``` to update Rust itself)
* ```rustc```: the compiler that turns your Rust code into executables
* ```cargo```: the Rust package and dependency manager, as well as build tool

## Project setup

We can use ```cargo``` to create a new project:

```
cargo new --bin rustnish
```

That creates a new folder "rustnish" (the project name) and the ```--bin```
option tells cargo to create a standalone application (instead of a library).

## Running the project

Cargo has created a "Hello World!" example in src/main.rs in our project. We
can execute it like this:

```
$ cargo run
   Compiling rustnish v0.1.0 (file:///home/klausi/workspace/rustnish)
    Finished dev [unoptimized + debuginfo] target(s) in 0.26 secs
     Running `target/debug/rustnish`
Hello, world!
```

Success! With cargo you don't have to think about compiler commands or anything -
 whenever you modify your source files just execute ```cargo run``` and it
will detect file changes and compile everything for you. A big thank you to the
Rust community at this point for providing such excellent tooling with the
language itself!

## Picking an editor

[areweideyet.com](https://areweideyet.com/) lists editors and Integrated
Development Environments (IDE) for Rust. I chose [Atom](https://atom.io/)
because it seems to have the most IDE features. Make sure to install all the
additional packages on
[https://areweideyet.com/#atom](https://areweideyet.com/#atom). Also make sure
to install ```rustfmt``` which can automatically format your code:

```
cargo install rustfmt
```

It is crucial to have good language support for navigating around in source
code. While working with Atom I really missed the functionality to click on
functions or types to jump to their definition with Ctrl+Click. That works in
Netbeans for example for Java and PHP and I would be really grateful if
somebody could show me how to do that in Atom and Rust.

## Installing a HTTP library

After a bit of research I found [Hyper](https://hyper.rs/) for Rust. That will
give us a client and server library to deal with HTTP requests so that we don't
need to parse HTTP requests ourselves.

Edit Cargo.toml in your project and add a dependency line:

```toml
[dependencies]
hyper = "0.10.9"
```

The next time you execute ```cargo run``` it will fetch the Hyper dependency
for you, compile everything and then run your program. Super easy!

## Namespaces and modules (crates)

Let's walk through the Rust code in src/main.rs that can be found on
[Github](https://github.com/klausi/rustnish).

Rust has a full namespace and module system and makes importing libraries
straightforward:

```rust
extern crate hyper;
use hyper::server::{Server, Request, Response};
```

That tells the compiler to use the Hyper library and import the "Server" type
for example to make use of it in our program. "Server" is a complex type called
a ```struct``` in Rust (comparable to a class in PHP/Java, but without
inheritance and instead a trait system). I'm still confused by all the Rust
terminology because an instance of a struct is not called on object. But there
are also trait objects in Rust, so the term is in use and a different concept
on its own. In general it feels like Rust is overloaded with concepts that you
can learn about. Like in C++.

## Writing an HTTP server

There must not be any expressions in the global scope in Rust, all logic has to
live in functions. Coming from a scripting language this was [the first thing
I got
wrong](http://stackoverflow.com/questions/41086033/how-do-i-start-a-web-server-i
n-rust-with-hyper) :-)

The main() function is the entry point for the program execution:

```rust
fn main() {}
```

Rust uses the fn keyword to define a function. "fn" instead of "function" is an
optimization to quicker write programs, but of course we know that we should
optimize for humans reading programs faster instead. "fn" is not a natural word
and harder to recognize for newcomers. (General programming rant: never
abbreviate things because somebody will have to read and understand this.)

```rust
fn main() {
    let server = Server::http("127.0.0.1:9090").unwrap();
}
```

We start an HTTP server on localhost listening on port 9090. Our test program
should only be reachable from our own computer, so localhost only seems
appropriate.

The ```unwrap()``` call here tells the compiler to ignore errors that could
happen when we try to bind our server to the interface/port. For example
another service could occupy that port already. In that case the unwrap() call
will raise a panic (comparable to a runtime exception) and our program will
terminate.

Rust does not have an exception system like in PHP/Java. All functions need to
return any outcome in their return type, that's why return types in Rust are
often complex types that express multiple different result possibilities a
function can have.

```rust
fn main() {
    let server = Server::http("127.0.0.1:9090").unwrap();
    let _guard = server.handle(pipe_through);
    println!("Listening on http://127.0.0.1:9090");
}

fn pipe_through(request: Request, mut response: Response) {}
```

Next we tell the server instance which of our function will handle requests by
passing in the function name "pipe_through". It receives an HTTP request
instance and a mutable HTTP response instance that we will populate later.

If a function returns something in Rust you can't ignore it, so we need this
superfluous unused variable here. Starting it with "_" tells the compiler to
ignore it.

The Hyper server will now listen in the background for incoming requests while
we write an output message in parallel that the server is running.
```println!()``` is a macro in Rust (yet another concept), but you can think of
it as just a function call with variable arguments for now.

## Handling HTTP requests

Let's look at the pipe_trough() function that will handle requests:

```rust
fn pipe_through(request: Request, mut response: Response) {
    let path = match request.uri {
        RequestUri::AbsolutePath(p) => p,
        RequestUri::AbsoluteUri(url) => url.path().to_string(),
        RequestUri::Authority(p) => p,
        RequestUri::Star => "*".to_string(),
    };
    let host = match request.headers.get::<Host>() {
        None => {
            return error_page("No host header in request".to_string(),
                              StatusCode::BadRequest,
                              response)
        }
        Some(h) => h,
    };
```

We us pattern matching with the ```match``` keyword here which works a bit like
switch statements in other languages but seems to be more powerful. That way we
can build variables from the different possible values of the incoming request
and also terminate early if the HTTP Host header is missing for example (with
our custom error_page() functions defined later).

Do you see all the ```to_string()``` weirdness here? Apparently statically
defined strings in quotes are not of the String type and you need to tell Rust
to allocate memory for them if you want to use them where String is expected.
While writing this code I never knew when that must be done, so I just throw
lots of ```to_string()``` calls in whenever the compiler complains about a type
mismatch.

```rust
let hostname = host.hostname.to_string();
let protocol = "http://".to_string();
let url_string = protocol + &hostname + &path;
```

A simple task such as string concatenation is really complicated in Rust. Why
is the protocol variable the only one without the reference operator? Why do I
have to convert the hostname variable to a string first if it is a reference
already? Same spiel as before: randomly throwing in ```to_string()``` and "&"
operators until the compiler shuts up and we have our desired result in the
url_string variable.

```rust
let client = Client::new();
let request_builder = client
    .request(request.method, url)
    .headers(request.headers.clone());
let mut upstream_response = request_builder.send().unwrap();
```

Using the Hyper HTTP client was pretty straightforward. The only thing that I
don't understand is the ```clone()``` call here. I just want to forward the
incoming request headers to the upstream request I'm building. They are of the
same type, so why do I have to clone them? Maybe this has something to do with
the Rust ownership model, but the compiler errors were not clear on that.

```rust
*response.status_mut() = upstream_response.status;
*response.headers_mut() = upstream_response.headers.clone();
io::copy(&mut upstream_response, &mut response.start().unwrap()).unwrap();
```

OK, I admit that in those last 3 lines I really have no idea anymore what I'm
doing. I have to use the "*" operator because there is no other way to modify
the response instance. Headers are weird again of course, so we do some cloning
for good measure. The IO copying is moving over the HTTP response body which is
represented by streams (which makes sense, you can have large data bodies). It
feels wrong to use io from the standard library for this when there really
should be a method
response.use_this_output_stream_as_your_output_stream(&stream).

And yes, there are way too many ```unwrap()``` calls in my code, indicating
that there is lots of error handling missing and this application will raise
lots of panics when given invalid data.

## Conclusion

Rust is one of the most powerful languages I have seen, giving you lots of
tools to write safe code. I love the strictness of the compiler that reminds
you of mistakes that you overlooked. The type and trait system is great, as
well as the explicit handling of mutable vs. immutable variables. This style is
definitely the future of programming.

That being said Rust is complex to use. I struggled a lot with going in circles
between my code, the API documentation and other example code. At times I felt
like working for the compiler and not for the sake of making progress in my
program. It annoys me that I wrote code that I don't fully understand, but that
is probably the case for every beginner in a new language :-)

A big thank you to the Hyper library authors for their powerful HTTP server and
client API. While I had some minor complaints the exposed API surface felt
exactly right to be productive fairly quickly.

Stay tuned for future blog posts where I plan to explore Configuration file
handling, Automated Testing, Structuring larger programs, Benchmarking and more.
