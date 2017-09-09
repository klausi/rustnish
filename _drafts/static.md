---
title: "Static variables made thread-safe in Rust"
layout: post
---

When writing [integration tests for my Rustnish reverse proxy project]({{
site.baseurl }}{% post_url 2017-05-25-writing-integration-tests-in-rust %}) I
have hard-coded port numbers in tests. This is not ideal because it is hard to
keep track of which port numbers have already been used and which ones are
available when writing a new test. Because Rust's test runner [executes test
cases in
parallel](https://doc.rust-lang.org/book/second-edition/ch11-02-running-tests.ht
ml#running-tests-in-parallel-or-consecutively) it is important to coordinate
which test uses which ports so that there are no clashes that break the tests.

One obvious solution to this problem would be to disable parallel test
execution with `cargo test -- --test-threads=1`. But we want to cover program
and test isolation with our test so this is not really an option.


## A naive try

The basic idea is to have a function `get_free_port()` that hands out port
numbers incrementally and is called by tests:

```rust
pub fn get_free_port() -> u16 {
    static mut PORT_NR: u16 = 9090;
    PORT_NR += 1;
    PORT_NR
}
```

We initialize with the number 9090 here and return an incremented number for
each call. The compiler doesn't seem to like it:

```
error[E0133]: use of mutable static requires unsafe function or block
  --> tests/common/mod.rs:99:5
   |
99 |     PORT_NR += 1;
   |     ^^^^^^^ use of mutable static
```

The compiler is saving me from a race condition here. Since tests are executed
concurrently 2 tests could enter this function at the same time. One increments
the port number, but before returning the operating system hands over execution
to the second test thread which also increments the port number. Now both calls
suddenly would return the same port number, which is exactly what we want to
avoid.

We need to isolate the calls to this function or access to the static shared
variable. In Java we would use the `synchronize` keyword on the function
definition to ensure that only one thread can enter it at a time. But Rust uses
more primitive synchronization constructs.


## Protecting static variables with AtomicUsize

The standard library has some [good documentation about synchronized atomic
access](https://doc.rust-lang.org/std/sync/atomic/) that we can use.

```rust
pub fn get_free_port() -> u16 {
    static PORT_NR: AtomicUsize = ATOMIC_USIZE_INIT;

    PORT_NR.compare_and_swap(0, 9090, Ordering::SeqCst);
    PORT_NR.fetch_add(1, Ordering::SeqCst) as u16
}
```

This works, but is a bit annoying:

1. We have to initialize the static variable with `ATOMIC_USIZE_INIT` instead
of our desired value 9090. If you try

   ```rust
   static PORT_NR: AtomicUsize = AtomicUsize::new(9090);
   ```

   then the compiler will complain:

   ```
   error: const fns are an unstable feature
      --> tests/common/mod.rs:98:35
      |
   98 |     static PORT_NR: AtomicUsize = AtomicUsize::new(9090);
      |                                   ^^^^^^^^^^^^^^^^^^^^^^
      |
      = help: in Nightly builds, add `#![feature(const_fn)]` to the crate attributes to enable
   ```

   We don't want to depend on the nightly compiler, so this is not possible
   right now.

2. The `compare_and_swap()` call is only necessary because we could not
   directly initialize our value to 9090. It is executed on every call to
   `get_free_port()` and is just a waste of execution time.

3. I have no idea what `Ordering::SeqCst` means. [The
   documentation](https://doc.rust-lang.org/std/sync/atomic/enum.Ordering.html)
   says that this variant is the most restrictive one but I don't know if this is
   necessary or ideal in my use case. I'm using it because it is used in the docs
   example  ¯\\\_(ツ)\_/¯

4. We have to cast to `u16` in the end because there is only an `AtomicUsize`
   type but no `AtomicU16`.


## Conclusion

Rust is great at detecting race conditions at compile time and helps you do the
right thing with static variables. The solution to synchronize concurrent
access with atomics feels a bit clumsy and there might be a better way that I
have not discovered yet.
