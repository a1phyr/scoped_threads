# Scoped Threads

[![Crates.io](https://img.shields.io/crates/v/scoped_threads.svg)](https://crates.io/crates/scoped_threads)
[![Docs.rs](https://docs.rs/scoped_threads/badge.svg)](https://docs.rs/scoped_threads/)

Lightweight, safe and idiomatic scoped threads

This crate provides a scoped alternative to `std::thread`, ie threads that can
use non-static data, such as references to the stack of the parent thread. It
mimics `std`'s thread interface, so there is nothing to learn to use this crate.
There is a meaningful difference, though: a dropped `JoinHandle` joins the
spawned thread instead of detaching it, to ensure that borrowed data is still
valid. Additionnally, this crate does not redefine types and functions that are
not related to threads being scoped, such as `thread::park`.

It is lightweight in the sense that it does not uses expensive thread
synchronisation such as `Arc`, locks or channels.

This crate's API is very unlikely to change and can be considered stable, but
the crate will only reach 1.0 when unstable feature `thread_spawn_unchecked` is
stabilized.

### Why `scoped_threads` and not `crossbeam_utils::scope` ?

- You might not want to bring extra ShardedLocks, CachePadded, etc with your
scoped threads
- The `scope` function works great but it feels a bit weird and is not flexible
- Synchronisation primitives such as `Arc` and `Mutex` are extensively used and
bring a significant overhead


## License

Licensed under either of

* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
