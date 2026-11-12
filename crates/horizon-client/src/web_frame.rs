//! The browser build's work between frames: what the desktop runs on threads.
//!
//! One thread runs it all, in this order each turn of the page's loop: the
//! connections' tasks (logins and the client's reading, `horizon_thread`'s local
//! tasks) and the network's turn (`lan::io_pool`), a few rounds so a packet and its
//! answer cross in the same frame; the single-player server (`server`); then the
//! job pool's chunk work for what is left of the budget.

use web_time::{Duration, Instant};

/// How long the work between two frames may take: about half a 60 Hz frame.
const BUDGET: Duration = Duration::from_millis(8);
/// Rounds of connection work at most per turn.
const ROUNDS: usize = 16;

/// Runs one turn: the connections, `server`, then chunk work.
pub fn run(server: impl FnOnce()) {
    let started = Instant::now();
    connections(started);
    server();
    connections(started);
    let left = BUDGET.saturating_sub(started.elapsed());
    horizon_threads::jobs::pump(left.max(Duration::from_millis(1)));
}

/// The connections' tasks and the network's turn, until nothing waits or the
/// budget is spent.
fn connections(started: Instant) {
    for _ in 0..ROUNDS {
        let pending = horizon_thread::run_local_tasks();
        crate::lan::io_pool_pump();
        if pending == 0 || started.elapsed() >= BUDGET {
            break;
        }
    }
}
