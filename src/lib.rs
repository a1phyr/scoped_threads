//! Lightweight, safe and idiomatic scoped threads
//!
//! This crate provides a scoped alternative to `std::thread`, ie threads that
//! can use non-static data, such as references to the stack of the parent
//! thread. It mimics `std`'s thread interface, so there
//! is nothing to learn to use this crate. There is a meaningful difference,
//! though: a dropped `JoinHandle` joins the spawned thread instead of
//! detaching it, to ensure that borrowed data is still valid. Additionnally,
//! this crate does not redefine types and functions that are not related to
//! threads being scoped, such as [`thread::park`].
//!
//! It is lightweight in the sense that it does not uses expensive thread
//! synchronisation such as `Arc`, locks or channels. It does not even uses
//! atomics !
//!
//! This API beeing nearly the same as `std`'s and this documentation having
//! no intention to be a copy of [`std`'s documentation on threads], you'll
//! find much more details in the latter.
//!
//! [`thread::park`]: https://doc.rust-lang.org/std/thread/fn.park.html
//! [`std`'s documentation on threads]: https://doc.rust-lang.org/std/thread/
//!
//! ## Cargo features
//!
//! This crate exposes a feature `nightly`, which uses unstable functions from
//! `std`. It changes some facets of the crate's implementation to remove the
//! two required memory allocations.
//!
//! ## Example
//!
//! Share a `Mutex` without `Arc`
//!
//! ```
//! use std::sync::Mutex;
//!
//! // This struct is not static, so it cannot be send in standard threads
//! struct Counter<'a> {
//!     count: &'a Mutex<i32>,
//!     increment: i32,
//! }
//!
//! let count = Mutex::new(0);
//!
//! let counter = Counter {
//!     count: &count,
//!     increment: 2,
//! };
//!
//! // Send the Counter in another thread
//! let join_handle = scoped_threads::spawn(move || {
//!     *counter.count.lock().unwrap() += counter.increment;
//! });
//!
//! // Wait for the spawned thread to finish
//! join_handle.join();
//!
//! assert_eq!(*count.lock().unwrap(), 2);
//! ```

#![cfg_attr(feature = "nightly", feature(thread_spawn_unchecked), allow(unused_imports))]

#![warn(
    missing_docs,
    missing_debug_implementations,
)]

use std::{
    any::Any,
    fmt,
    io,
    marker::PhantomData,
    mem::{self, ManuallyDrop, MaybeUninit},
    ptr,
    thread,
};


#[cfg(not(feature = "nightly"))]
struct AssertSend<T>(T);
#[cfg(not(feature = "nightly"))]
unsafe impl<T> Send for AssertSend<T> {}


/// Thread factory, which can be used in order to configure the properties of a
/// new thread.
///
/// Methods can be chained on it in order to configure it.
///
/// It can be used to specify a name and a stack size for the new thread, or to
/// recover from a failure to launch it.
///
/// See [`std::thread::Builder`](https://doc.rust-lang.org/std/thread/struct.Builder.html)
/// for more details.
///
/// # Example
///
/// ```
/// let builder = scoped_threads::Builder::new();
/// builder.name("Scoped thread".into())
///     .stack_size(100 * 1024)
///     .spawn(|| {
///         // thread code
///     })?;
/// # Ok::<(),std::io::Error>(())
/// ```
#[must_use = "Builders do nothing until used to `spawn` a thread"]
pub struct Builder {
    inner: thread::Builder,
}

impl Builder {
    /// Creates a new `Builder`.
    ///
    /// # Example
    ///
    /// ```
    /// let builder = scoped_threads::Builder::new();
    /// builder.spawn(|| {
    ///     // thread code
    /// })?;
    /// # Ok::<(),std::io::Error>(())
    /// ```
    pub fn new() -> Builder {
        thread::Builder::new().into()
    }

    /// Sets the name of the thread.
    ///
    /// The name must not contain null bytes (`\0`).
    ///
    /// # Example
    ///
    /// ```
    /// use std::thread;
    ///
    /// let builder = scoped_threads::Builder::new().name("foo".into());
    /// let handle = builder.spawn(|| {
    ///     assert_eq!(thread::current().name(), Some("foo"));
    /// })?;
    ///
    /// assert!(handle.join().is_ok());
    /// # Ok::<(),std::io::Error>(())
    /// ```
    #[inline]
    pub fn name(self, name: String) -> Builder {
        self.inner.name(name).into()
    }

    /// Sets the size of the stack (in bytes) for the new thread.
    ///
    /// The actual stack size may be greater than this value if the platform
    /// specifies a minimal stack size.
    ///
    /// # Example
    ///
    /// ```
    /// let builder = scoped_threads::Builder::new().stack_size(32 * 1024);
    /// ```
    #[inline]
    pub fn stack_size(self, size: usize) -> Builder {
        self.inner.stack_size(size).into()
    }

    /// Consume the builder to spawn a new thread.
    ///
    /// Unlike in `std`, the closure and the return type do not need a `'static`
    /// lifetime.
    ///
    /// # Errors
    ///
    /// This method yields an io::Result to capture any failure to create the
    /// thread at the OS level.
    ///
    /// # Panics
    ///
    /// Panics if a thread name was set and it contained null bytes.
    ///
    /// # Example
    ///
    /// ```
    /// let builder = scoped_threads::Builder::new();
    /// builder.spawn(|| {
    ///     // thread code
    /// })?;
    /// # Ok::<(),std::io::Error>(())
    /// ```
    pub fn spawn<'a, F, T>(self, f: F) -> io::Result<JoinHandle<'a, T>>
    where
        F: FnOnce() -> T + Send + 'a,
        T: Send + 'a
    {
        #[cfg(feature = "nightly")]
        let handle = unsafe {
            ManuallyDrop::new(self.inner.spawn_unchecked(f)?)
        };

        #[cfg(not(feature = "nightly"))]
        let (handle, ret) = {
            let mut ret = Box::new(MaybeUninit::uninit());
            let ret_ptr = AssertSend(ret.as_mut_ptr());

            let closure = move || {
                let result = f();

                // Safety: JoinHandle do not use this pointer before the thread
                // is joined
                unsafe {
                    ptr::write(ret_ptr.0, result);
                }
            };

            let closure: Box<dyn FnOnce() + Send + 'a> = Box::new(closure);
            let closure: Box<dyn FnOnce() + Send + 'static> = unsafe { mem::transmute(closure) };

            let handle = ManuallyDrop::new(self.inner.spawn(closure)?);
            (handle, ret)
        };

        Ok(JoinHandle {
            handle,

            #[cfg(not(feature = "nightly"))]
            ret,

            marker: PhantomData,
        })
    }
}

impl From<thread::Builder> for Builder {
    #[inline]
    fn from(builder: thread::Builder) -> Builder {
        Builder {
            inner: builder,
        }
    }
}

impl fmt::Debug for Builder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}


/// Spawns a new thread.
///
/// Unlike in `std`, the closure and the return type do not need a `'static`
/// lifetime. However, the spawn thread cannot be detached and will join on the
/// current thread when its [`JoinHandle`] is dropped. This ensures the safety
/// of the unbounded lifetime.
///
/// # Panics
///
/// Panics if the OS fails to create a thread; use [`Builder::spawn`] to recover
/// from such errors.
///
/// [`Builder::spawn`]: struct.Builder.html
///
/// # Example
///
/// ```
/// let handle = scoped_threads::spawn(|| {
///     // thread code
/// });
/// ```
///
/// ## Note
///
/// These ways to call `spawn` immediatly drop the returned [`JoinHandle`],
/// which you do want want:
///
/// ```
/// scoped_threads::spawn(|| { /* thread code */ });
/// let _ = scoped_threads::spawn(|| { /* thread code */ });
/// ```
///
/// Drop or `join` it instead, or use this to implicitly drop it at the end of
/// the scope:
///
/// ```
/// let _handle = scoped_threads::spawn(|| { /* thread code */ });
/// ```
///
/// [`JoinHandle`]: struct.JoinHandle.html
pub fn spawn<'a, F, T>(f: F) -> JoinHandle<'a, T>
where
    F: FnOnce() -> T + Send + 'a,
    T: Send + 'a,
{
    Builder::new().spawn(f).expect("failed to spawn thread")
}


/// Enables to join on a spawned thread.
///
/// Unlike in `std`, this handle joins on the spawned thread when dropped. This
/// ensures that it is safe to send non-static values. The lifetime parameter
/// of this type represents such values.
///
/// This struct is created by the [`spawn`] function and the [`Builder::spawn`]
/// method.
///
/// Note that unlike [`std::thread::JoinHandle`], this type does not implement
/// platform-specific traits.
///
/// # Example
///
/// ```
/// use std::thread::sleep;
/// use std::time::Duration;
///
/// let join_handle = scoped_threads::spawn(|| {
///     // Some expensive computation
///     sleep(Duration::from_millis(50));
/// });
///
/// // Wait the spawned thread to be joined
/// join_handle.join();
/// ```
///
/// [`spawn`]: fn.spawn.html
/// [`Builder::spawn`]: struct.Builder.html#method.spawn
/// [`std::thread::JoinHandle`]: https://doc.rust-lang.org/std/thread/struct.JoinHandle.html
#[must_use = "Unused JoinHandle are immediatly dropped and joined"]
pub struct JoinHandle<'a, T: 'a> {
    #[cfg(not(feature = "nightly"))]
    handle: ManuallyDrop<thread::JoinHandle<()>>,
    #[cfg(feature = "nightly")]
    handle: ManuallyDrop<thread::JoinHandle<T>>,

    #[cfg(not(feature = "nightly"))]
    ret: Box<MaybeUninit<T>>,

    marker: PhantomData<&'a ()>,
}

#[cfg(not(feature = "nightly"))]
unsafe impl<T> Send for JoinHandle<'_, T> {}
#[cfg(not(feature = "nightly"))]
unsafe impl<T> Sync for JoinHandle<'_, T> {}

impl<T> JoinHandle<'_, T> {
    /// Joins the associated thread.
    ///
    /// # Safety
    ///
    /// This function can only be called once
    #[inline]
    unsafe fn join_unchecked(&mut self) -> Result<T, Box<dyn Any + Send>> {
        let _res = ManuallyDrop::take(&mut self.handle).join()?;

        #[cfg(feature = "nightly")]
        return Ok(_res);

        #[cfg(not(feature = "nightly"))]
        return Ok(self.ret.as_ptr().read());
    }

    /// Waits for the associated thread to finish.
    ///
    /// If the child thread panics, `Err` is returned with the parameter given
    /// to panic.
    ///
    /// # Panics
    ///
    /// This function may panic on some platforms if a thread attempts to join
    /// itself or otherwise may create a deadlock with joining threads.
    ///
    /// # Examples
    ///
    /// The spawned thread returns a value:
    ///
    /// ```
    /// let join_handle = scoped_threads::spawn(|| {
    ///     42
    /// });
    ///
    /// let result = join_handle.join().unwrap();
    /// assert_eq!(result, 42);
    /// ```
    ///
    /// The spawned thread panics:
    ///
    /// ```
    /// let join_handle = scoped_threads::spawn(|| {
    ///     panic!()
    /// });
    ///
    /// let result = join_handle.join();
    /// assert!(result.is_err());
    /// ```
    #[inline]
    pub fn join(self) -> Result<T, Box<dyn Any + Send>> {
        // `ManuallyDrop` ensures that `self` is not dropped if `join_unchecked`
        // panics, which is not the case with `mem::forget`
        let mut handle = ManuallyDrop::new(self);
        unsafe { handle.join_unchecked() }
    }

    /// Extracts a handle to the underlying thread.
    ///
    /// # Example
    ///
    /// ```
    /// let join_handle = scoped_threads::spawn(|| {
    ///     // thread code
    /// });
    ///
    /// let thread = join_handle.thread();
    /// println!("thread id: {:?}", thread.id());
    /// ```
    #[inline]
    pub fn thread(&self) -> &thread::Thread {
        self.handle.thread()
    }
}

impl<T> Drop for JoinHandle<'_, T> {
    #[inline]
    fn drop(&mut self) {
        // Ignore panics in remote thread
        let _ = unsafe { self.join_unchecked() };
    }
}

impl<T> fmt::Debug for JoinHandle<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("JoinHandle { .. }")
    }
}


#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    struct DropCounter<'a>(&'a Mutex<usize>);

    impl Drop for DropCounter<'_> {
        fn drop(&mut self) {
            *self.0.lock().unwrap() += 1;
        }
    }

    #[test]
    fn no_drop() {
        let count = Mutex::new(0);
        
        let _counter = crate::spawn(|| {
            DropCounter(&count)
        }).join().unwrap();
        
        assert_eq!(*count.lock().unwrap(), 0);
    }
}
