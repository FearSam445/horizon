//! The browser build's work between frames: what the desktop runs on threads.
//!
//! One thread runs it all, in this order each turn of the page's loop: the
//! connections' tasks (logins and the client's reading, `horizon_thread`'s local
//! tasks) and the network's turn (`lan::io_pool`), a few rounds so a packet and its
//! answer cross in the same frame; the single-player server (`server`); then the
//! job pool's chunk work for what is left of the budget.

use std::cell::Cell;
use web_time::{Duration, Instant};

/// How long the work between two frames may take: about half a 60 Hz frame.
const BUDGET: Duration = Duration::from_millis(8);
/// Rounds of connection work at most per turn.
const ROUNDS: usize = 16;
/// How often the console gets a line on the work between frames.
const REPORT_EVERY: Duration = Duration::from_secs(2);

/// Counts since the last report, for the console line.
#[derive(Clone, Copy, Default)]
struct Counts {
    turns: u32,
    tasks_pending: usize,
    jobs: usize,
    server_ms: f32,
}

thread_local! {
    static COUNTS: Cell<(Option<Instant>, Counts)> = const {
        Cell::new((None, Counts { turns: 0, tasks_pending: 0, jobs: 0, server_ms: 0.0 }))
    };
}

/// Runs one turn: the connections, `server`, then chunk work.
pub fn run(server: impl FnOnce()) {
    let started = Instant::now();
    let pending_before = connections(started);
    let server_start = Instant::now();
    server();
    let server_ms = server_start.elapsed().as_secs_f32() * 1000.0;
    let pending = connections(started).max(pending_before);
    let left = BUDGET.saturating_sub(started.elapsed());
    let jobs = horizon_threads::jobs::pump(left.max(Duration::from_millis(1)));
    report(pending, jobs, server_ms);
}

/// The connections' tasks and the network's turn, until nothing waits or the
/// budget is spent. Returns the tasks still waiting.
fn connections(started: Instant) -> usize {
    let mut pending = 0;
    for _ in 0..ROUNDS {
        pending = horizon_thread::run_local_tasks();
        crate::lan::io_pool_pump();
        if pending == 0 || started.elapsed() >= BUDGET {
            break;
        }
    }
    pending
}

/// The page's memory for the game: wasm's linear memory, which only grows.
fn memory_mib() -> usize {
    #[cfg(target_arch = "wasm32")]
    return core::arch::wasm32::memory_size(0) * 65536 >> 20;
    #[allow(unreachable_code)]
    0
}

/// A console line every [`REPORT_EVERY`]: turns, connection tasks still waiting,
/// chunk jobs run and the server's time, so a stalled world shows what stopped.
fn report(pending: usize, jobs: usize, server_ms: f32) {
    COUNTS.with(|cell| {
        let (since, mut c) = cell.get();
        c.turns += 1;
        c.tasks_pending = pending;
        c.jobs += jobs;
        c.server_ms += server_ms;
        let now = Instant::now();
        match since {
            None => cell.set((Some(now), c)),
            Some(at) if now.duration_since(at) >= REPORT_EVERY => {
                log::info!(
                    "Horizon web: {} turns, {} connection tasks waiting, {} chunk jobs, server {:.0} ms, memory {} MiB",
                    c.turns,
                    c.tasks_pending,
                    c.jobs,
                    c.server_ms,
                    memory_mib()
                );
                cell.set((Some(now), Counts::default()));
            }
            Some(_) => cell.set((since, c)),
        }
    });
}
