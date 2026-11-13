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
/// A 60 Hz frame. winit's loop turns after every input event too (each mouse
/// move), so the budget is the frame's, shared by all the turns in it: else a
/// frame with ten pointer moves ran ten budgets of chunk work.
const FRAME: Duration = Duration::from_micros(16_600);
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
    /// [`horizon_threads::jobs::run_here`] at the last report.
    jobs_before: usize,
    server_ms: f32,
    /// The longest server step, frame drawn and turn (ms).
    server_max: f32,
    render_max: f32,
    turn_max: f32,
}

thread_local! {
    static COUNTS: Cell<(Option<Instant>, Counts)> = const {
        Cell::new((None, Counts { turns: 0, tasks_pending: 0, jobs: 0, jobs_before: 0, server_ms: 0.0, server_max: 0.0, render_max: 0.0, turn_max: 0.0 }))
    };
    /// The frame this turn is in: when it started and the work time spent in it.
    static WINDOW: Cell<Option<(Instant, Duration)>> = const { Cell::new(None) };
}

/// Whether the frame's budget is shared by its turns ([`FRAME`]): on, but a test
/// can turn it off to compare (horizon-web's `set_frame_budget`).
static SHARED_BUDGET: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Turns the shared frame budget on or off (for measuring).
pub fn set_shared_budget(on: bool) {
    SHARED_BUDGET.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Runs one turn: the connections, `server`, then chunk work.
pub fn run(server: impl FnOnce()) {
    let started = Instant::now();
    let shared = SHARED_BUDGET.load(std::sync::atomic::Ordering::Relaxed);
    let (frame, spent) = match WINDOW.with(Cell::get) {
        Some((frame, spent)) if shared && started.duration_since(frame) < FRAME => (frame, spent),
        _ => (started, Duration::ZERO),
    };
    let budget = BUDGET.saturating_sub(spent);
    // Chunk jobs (wherever this turn pumps them) stop once the frame's budget is
    // spent; one still runs in each frame, so the world keeps loading.
    horizon_threads::jobs::frame_until(shared.then_some(started + budget), frame == started);
    let pending_before = connections(started, budget);
    let server_start = Instant::now();
    server();
    let server_ms = server_start.elapsed().as_secs_f32() * 1000.0;
    let pending = connections(started, budget).max(pending_before);
    let left = budget.saturating_sub(started.elapsed());
    horizon_threads::jobs::pump(left.max(Duration::from_millis(1)));
    horizon_threads::jobs::frame_until(None, false);
    let jobs = horizon_threads::jobs::run_here();
    WINDOW.with(|w| w.set(Some((frame, spent + started.elapsed()))));
    report(pending, jobs, server_ms, started.elapsed().as_secs_f32() * 1000.0);
}

/// How long a frame took to draw: its longest goes in the status line.
pub fn note_render(took: Duration) {
    COUNTS.with(|cell| {
        let (since, mut c) = cell.get();
        c.render_max = c.render_max.max(took.as_secs_f32() * 1000.0);
        cell.set((since, c));
    });
}

/// The connections' tasks and the network's turn, until nothing waits or the
/// budget is spent. Returns the tasks still waiting.
fn connections(started: Instant, budget: Duration) -> usize {
    let mut pending = 0;
    for _ in 0..ROUNDS {
        pending = horizon_thread::run_local_tasks();
        crate::lan::io_pool_pump();
        if pending == 0 || started.elapsed() >= budget {
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
fn report(pending: usize, jobs: usize, server_ms: f32, turn_ms: f32) {
    COUNTS.with(|cell| {
        let (since, mut c) = cell.get();
        c.turns += 1;
        c.server_max = c.server_max.max(server_ms);
        c.turn_max = c.turn_max.max(turn_ms);
        c.tasks_pending = pending;
        c.jobs = jobs;
        c.server_ms += server_ms;
        let now = Instant::now();
        match since {
            None => cell.set((Some(now), c)),
            Some(at) if now.duration_since(at) >= REPORT_EVERY => {
                log::info!(
                    "Horizon web: {} turns, {} connection tasks waiting, {} chunk jobs, server {:.0} ms, memory {} MiB",
                    c.turns,
                    c.tasks_pending,
                    c.jobs - c.jobs_before,
                    c.server_ms,
                    memory_mib()
                );
                // What the longest frames were made of: the longest turn, server
                // step and drawing, and each chunk job kind's count, total and longest.
                let times: Vec<String> = horizon_client_render::streaming::job_times()
                    .iter()
                    .map(|(kind, n, total, max)| format!("{kind} {n}x {total:.0}/{max:.0}"))
                    .collect();
                log::info!(
                    "Horizon web: longest turn {:.0} ms, server step {:.0}, drawing {:.0}; jobs (count total/longest ms): {}",
                    c.turn_max,
                    c.server_max,
                    c.render_max,
                    times.join(", ")
                );
                cell.set((
                    Some(now),
                    Counts {
                        jobs_before: jobs,
                        ..Counts::default()
                    },
                ));
            }
            Some(_) => cell.set((since, c)),
        }
    });
}
