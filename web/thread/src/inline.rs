//! `std::thread` for one thread: `spawn` and scoped threads run the closure at
//! once; `Builder::spawn` fails as std's does here.

use std::io;
use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::time::Duration;

pub use std::thread::{
    AccessError, LocalKey, Result, Thread, ThreadId, current, panicking, park, park_timeout,
    yield_now,
};

/// What a sleep does instead of waiting ([`set_idle`]).
static IDLE: std::sync::OnceLock<fn(Duration)> = std::sync::OnceLock::new();

/// Gives sleeps work to do: the page's only thread can't wait, and the work a
/// sleeping loop waits for (chunks, for one) is the work it must do itself.
/// The browser build sets the job pool's pump here at boot.
pub fn set_idle(work: fn(Duration)) {
    let _ = IDLE.set(work);
}

/// The page's only thread can't wait: a sleep does the idle work ([`set_idle`])
/// for at most `dur` instead, and returns at once without any.
pub fn sleep(dur: Duration) {
    if let Some(work) = IDLE.get() {
        work(dur);
    }
}

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
    /// Fails, as std's does on wasm32: a named thread is most often a worker that
    /// loops until it is told to stop, which would hold the page's only thread.
    /// Callers fall back to doing the work themselves; one that can't uses
    /// [`spawn`], which runs the closure at once.
    pub fn spawn<F, T>(self, _f: F) -> io::Result<JoinHandle<T>>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "no threads in the browser build",
        ))
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

/// Work that waits for something another part of the game does on the same
/// thread (a connection's next packet): a future polled again each turn of the
/// page's loop until it is done.
type Task = std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>;

thread_local! {
    static TASKS: std::cell::RefCell<Vec<Task>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Runs `task` from now on: a first poll at once, then each [`run_local_tasks`].
pub fn spawn_local(task: impl std::future::Future<Output = ()> + 'static) {
    let mut task: Task = Box::pin(task);
    if poll_once(&mut task).is_pending() {
        TASKS.with(|tasks| tasks.borrow_mut().push(task));
    }
}

/// Polls every task once; returns how many are still pending. A task spawned
/// while they run waits for the next call.
pub fn run_local_tasks() -> usize {
    let mut running = TASKS.with(|tasks| std::mem::take(&mut *tasks.borrow_mut()));
    running.retain_mut(|task| poll_once(task).is_pending());
    TASKS.with(|tasks| {
        let mut tasks = tasks.borrow_mut();
        running.append(&mut tasks);
        *tasks = running;
        tasks.len()
    })
}

fn poll_once(task: &mut Task) -> std::task::Poll<()> {
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    task.as_mut().poll(&mut context)
}
