//! `std::thread` for one thread: spawning runs the closure at once.

use std::io;
use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::time::Duration;

pub use std::thread::{
    AccessError, LocalKey, Result, Thread, ThreadId, current, panicking, park, park_timeout,
    yield_now,
};

/// Waiting is not possible on the page's only thread: a sleep returns at once.
/// A loop that sleeps until another thread acts must be made a step instead.
pub fn sleep(_dur: Duration) {}

/// `sleep_until`, which returns at once for the same reason as [`sleep`].
pub fn sleep_until<I>(_deadline: I) {}

/// One thread.
pub fn available_parallelism() -> io::Result<NonZeroUsize> {
    Ok(NonZeroUsize::MIN)
}

/// A finished "thread": its closure already ran.
#[derive(Debug)]
pub struct JoinHandle<T> {
    result: Option<T>,
    thread: Thread,
}

impl<T> JoinHandle<T> {
    pub fn join(mut self) -> Result<T> {
        Ok(self.result.take().expect("joined once"))
    }
    pub fn thread(&self) -> &Thread {
        &self.thread
    }
    pub fn is_finished(&self) -> bool {
        true
    }
}

/// Runs `f` now; [`JoinHandle::join`] gives its result.
pub fn spawn<F, T>(f: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    JoinHandle {
        result: Some(f()),
        thread: current(),
    }
}

/// A scope whose threads run when they are spawned.
pub struct Scope<'scope, 'env: 'scope> {
    scope: PhantomData<&'scope mut &'scope ()>,
    env: PhantomData<&'env mut &'env ()>,
}

/// A scoped thread that already ran.
pub struct ScopedJoinHandle<'scope, T> {
    result: Option<T>,
    thread: Thread,
    scope: PhantomData<&'scope ()>,
}

impl<'scope, T> ScopedJoinHandle<'scope, T> {
    pub fn join(mut self) -> Result<T> {
        Ok(self.result.take().expect("joined once"))
    }
    pub fn thread(&self) -> &Thread {
        &self.thread
    }
    pub fn is_finished(&self) -> bool {
        true
    }
}

impl<'scope, 'env> Scope<'scope, 'env> {
    pub fn spawn<F, T>(&'scope self, f: F) -> ScopedJoinHandle<'scope, T>
    where
        F: FnOnce() -> T + Send + 'scope,
        T: Send + 'scope,
    {
        ScopedJoinHandle {
            result: Some(f()),
            thread: current(),
            scope: PhantomData,
        }
    }
}

/// `std::thread::scope`: `f` runs with a scope whose threads run inline.
pub fn scope<'env, F, T>(f: F) -> T
where
    F: for<'scope> FnOnce(&'scope Scope<'scope, 'env>) -> T,
{
    let scope = Scope {
        scope: PhantomData,
        env: PhantomData,
    };
    f(&scope)
}

/// Thread settings; the name and stack size have nothing to apply to here.
#[derive(Debug, Default)]
pub struct Builder {
    _name: Option<String>,
}

impl Builder {
    pub fn new() -> Builder {
        Builder::default()
    }
    pub fn name(self, name: String) -> Builder {
        Builder { _name: Some(name) }
    }
    pub fn stack_size(self, _size: usize) -> Builder {
        self
    }
    pub fn no_hooks(self) -> Builder {
        self
    }
    pub fn spawn<F, T>(self, f: F) -> io::Result<JoinHandle<T>>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        Ok(spawn(f))
    }
    pub fn spawn_scoped<'scope, 'env, F, T>(
        self,
        scope: &'scope Scope<'scope, 'env>,
        f: F,
    ) -> io::Result<ScopedJoinHandle<'scope, T>>
    where
        F: FnOnce() -> T + Send + 'scope,
        T: Send + 'scope,
    {
        Ok(scope.spawn(f))
    }
}
