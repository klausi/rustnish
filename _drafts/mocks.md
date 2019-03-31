---
title: "Mocking in Rust with conditional compilation"
layout: post
---

When writing automated unit tests for your application you will probably need to use [mocks](https://en.wikipedia.org/wiki/Mock_object) at some point. Classical object-oriented programming languages such as PHP solve this with reflection where mock object types are created during test runtime. The code under test expects a certain interface or class and the test code passes mock objects that implement the interface or are a subclass.

Similar approaches exist in Rust where mock objects are used to test code that expects a trait type. There is a wonderful [Rust mock framework comparison](https://asomers.github.io/mock_shootout/) by Alan Somers that lists their features. The biggest problem with most of them as far as I can see is that they cannot mock a foreign `struct` you are using in your code. Rust does not have a concept of object inheritance for structs so there is no way to mimic a struct type from the standard library or an external crate.

One workaround for that is to refactor your code to use traits/generics instead of concrete structs. That way the implementation is agnostic to whatever struct is used - test code can pass mocks that satisfy the expected trait bounds. The downside is that you might have to invent traits which can make your code more complex just to satisfy test requirements.

Another solution is to use one of Rust's powerful meta programming capabilities: [conditional compilation](https://doc.rust-lang.org/reference/conditional-compilation.html).


## Conditional compilation for test types

This was quite a revelation to me and still blows my mind: you can just swap out a complete type during test runs! I first found this when looking at the [lru_time_cache crate](https://github.com/maidsafe/lru_time_cache) and the [test_double crate](https://github.com/pcsm/test_double).

Let's look at a concrete use case as I implemented for [the cache part](https://github.com/klausi/rustnish/blob/goal-11/src/cache.rs) of my Rustnish project (a fork of the mentioned lru_time_cache crate). The cache has an `insert()` and `len()` method defined like this:

```rust
use std::time::Instant;

/// Inserts a key-value pair into the cache.
pub fn insert(&mut self, key: Key, value: Value, expires: Instant) -> Option<Value> {
    self.remove_expired();
    // ...
    // Rest of function body omitted here.
}

/// Returns the size of the cache, i.e. the number of cached non-expired key-value pairs.
pub fn len(&self) -> usize {
    self.map
        .iter()
        .filter(|&(_, (_, t, _))| *t >= Instant::now())
        .count()
}
```

Whenever `len()` is called it has to go through all items in the cache and only count the non-expired ones (expired items are only removed in `insert()` calls).

How do we test this effectively? We could use `thread::sleep()` in test functions and check real time results, but that makes the unit test slow and dependent on thread time. How can we mock the `Instant` struct and associated methods instead? The secret sauce is this:

```rust
// During testing we use a mock clock to be time independent.
#[cfg(test)]
use fake_clock::FakeClock as Instant;
#[cfg(not(test))]
use std::time::Instant;
```

The `cfg` attribute is used here to swap in a mock `Instant` type whenever the tests are executed. During production compilation the normal type is used. Luckily in this case a [fake clock crate](https://github.com/maidsafe/fake_clock) already exists, so we don't even have to write the mock code and just use it:

```rust
fn sleep(time: u64) {
    use fake_clock::FakeClock;
    FakeClock::advance_time(time);
}

#[test]
fn expiration_time() {
    let time_to_live = Duration::from_millis(100);
    let mut lru_cache = super::LruCache::<usize, usize>::with_memory_size(10000);

    for i in 0..10 {
        assert_eq!(lru_cache.len(), i);
        let _ = lru_cache.insert(i, i, Instant::now() + time_to_live);
        assert_eq!(lru_cache.len(), i + 1);
    }

    sleep(101);
    let _ = lru_cache.insert(11, 11, Instant::now() + time_to_live);

    // All old items are expired, so only the last item must remain.
    assert_eq!(lru_cache.len(), 1);

    for i in 0..10 {
        assert!(!lru_cache.is_empty());
        assert_eq!(lru_cache.len(), i + 1);
        let _ = lru_cache.insert(i, i, Instant::now() + time_to_live);
        assert_eq!(lru_cache.len(), i + 2);
    }

    sleep(101);
    // All items are expired, so the cache must report being empty.
    assert_eq!(0, lru_cache.len());
    assert!(lru_cache.is_empty());
}
```

`FakeClock` exhibits the same methods as `Instant` from the standard library, so the compiler has no problem to use it as a drop-in replacement. We can manipulate the FakeClock from the outside and pretend that a certain amount of time has passed while we really just increase a counter. Super fast unit test, no waiting with a `thread::sleep()` needed!


## Integration tests not affected

"But Klausi!" you scream "Now your reverse proxy integration tests are broken because they will also run with the fake clock!"

No, because Rust compiles each [integration test](https://doc.rust-lang.org/rust-by-example/testing/integration_testing.html) as separate crate. It links it with your main crate, but only the integration test code has `#[cfg(test)]` mode on during that test run. The main crate will use the production `Instant` type and everything still works as before.


## Downsides of conditional compilation mocks

So far so good, but there are some downsides to consider with this approach:

* You can only swap in one mock implementation for all your test cases. Every test case shares the same mock code, so you need to come up with your own strategy if you need different mock behavior per test case.
* Integration tests become more import to have in addition to unit tests. The unit tests run with a complete fake type, so you don't even know if your code still compiles with the real type.
* If we would not have the fake_clock crate then we would have to write all the mock code ourselves, which is not trivial. This is probably the nature of mocking in general: complex type usage results in complex mock code to replace it.

As you can see there is great overlap with the general challenges of mocking, so don't take these as arguments against conditional compilation mocks specifically.


## Conclusion

Rust is super flexible and powerful on the topic of mocking. Conditional compilation is a next level opportunity that is simply missing in other programming languages.

But there are also maturity problems:
* There are 7 competing mocking frameworks, a sign that the Rust ecosystem has not figured out yet how to do it effectively for everybody.
* The [official Testing documentation](https://doc.rust-lang.org/1.33.0/book/ch11-00-testing.html) does not even mention mocking. I assume any larger Rust program will run into mocking use cases during tests, so this is lacking. Interestingly there is a mock use case in the [RefCell example documentation](https://doc.rust-lang.org/1.33.0/book/ch15-05-interior-mutability.html#a-use-case-for-interior-mutability-mock-objects).