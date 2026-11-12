# Horizon → single-file browser build

Goal: one `.html` file that boots when dropped into any Eaglercraft-style runner
(including the WASM Runner Controller), with the smallest possible RAM footprint.
The launcher is out of scope.

## Runner constraints (from WASM_Runner_Controller.html)
- The file is read with `FileReader.readAsText` and injected with `document.write`,
  so everything must be text: WASM is embedded as gzip + base64 and decoded at
  boot with `DecompressionStream`, then the base64 string is dropped so it can be
  garbage-collected.
- Runs inside a sandboxed iframe (CodeHS): assume no SharedArrayBuffer, so the
  build is single-threaded; assume IndexedDB/localStorage may throw, so wrap every
  access in try/catch and fall back to in-memory storage.
- Pointer lock is faked by the runner, so read `movementX/Y` from mouse events
  and don't treat lock failure as pause.
- Gamepad shim dispatches `keydown/keyup` on `window` (with `code`, `key`,
  `keyCode`) and `mousedown/mouseup/mousemove/wheel` on
  `canvas._eaglercraftX_canvas_element`. Our canvas must carry that class, and
  input must come from these mouse/keyboard events, not winit's pointer events.

## Game files
Minecraft's textures, sounds and data can't be embedded. On first boot the page
asks for the player's own prepared files (`horizon-prepare` output, zipped) and
caches them when storage is available.

## Native blockers found by `cargo check --target wasm32-unknown-unknown`
| Dependency | Pulled in by | Plan on wasm32 |
|---|---|---|
| mimalloc / libmimalloc-sys | horizon-threads (global allocator) | cfg-gate; use Rust's default dlmalloc (small, no C) |
| rayon | horizon-client-lib | run sequentially on wasm (single thread) |
| mio (net) | horizon-client-lib | stub out; multiplayer later via WebSocket relay |
| ureq + ring/rustls (HTTPS) | client-lib, client-render, cosmetics, chat-filter, server-files | cfg-gate out (skins/cosmetics/updates off) |
| cpal (audio) | client-lib/render (`audio` feature) | disable; WebAudio backend later |
| gilrs / hidapi (controllers) | client-lib (`controllers` feature) | disable; the runner turns the controller into keyboard/mouse |
| wgpu vulkan/metal | client crates | add `webgl` backend (WebGL2: widest support, lower RAM than WebGPU) |
| winit | client | keep for the window/event loop, feed input from DOM events |
| filesystem (saves) | world/chunk_io | in-memory VFS persisted to IndexedDB when allowed |

## Biggest design change: one thread
Single player runs a second `Game` on a "Server thread" (`game/integrated.rs`)
and the window joins it through memory as a client. With no threads in the
runner, the web build must tick the integrated server cooperatively on the main
thread (a fixed time budget per frame, e.g. server tick then client frame), and
the chunk worker pool (`streaming.rs`) must become a per-frame job queue.

## Low-RAM settings for the web build
- `opt-level = "z"`, LTO, `panic = "abort"`, `wasm-opt -Oz`.
- Default render distance 4–6 chunks; distant terrain, HDR and shaders off.
- Integrated server and client share one thread; fewer loaded chunks on the server too.
- Texture atlas without extra mip levels; drop GPU-side copies once uploaded.

## Decisions so far
- **Target: `wasm32-unknown-unknown`.** winit's web backend and wasm-bindgen only
  support it (wasm-bindgen refuses WASI), so `std::fs`, `Instant::now`,
  `SystemTime::now` and threads don't work there and are supplied by the game:
  - Clock: `web_time::Instant` everywhere (std::time on the desktop);
    `horizon_clock::system_now()` for `SystemTime::now()` (web/clock).
  - Files: `horizon_fs` (web/fs): std::fs on the desktop, an in-memory file
    system with read-only zip mounts in the browser. Rewritten by
    `web/tools/webfs.py`.
  - Crates that can't run in a page get stand-ins under their own names
    (web/shims): mimalloc, ureq, mio, fs4, arboard.
- **Graphics: WebGL2** through wgpu's `webgl` backend, with
  `fragile-send-sync-non-atomic-wasm` (one thread, no atomics).
- **Threads:** no SharedArrayBuffer in a runner's sandboxed iframe, so no real
  threads. `horizon_thread` (web/thread): `spawn` and scoped threads run their
  closure inline; `Builder::spawn` fails, as std's does on wasm32, so workers
  take their callers' no-thread fallbacks. The single-player server runs in the
  window's frames (`Integrated::step`), and the job pool's chunk work is pumped
  from the main loop (`jobs::pump`, 8 ms a frame).
- **WebGL fences only signal between tasks:** nothing may block waiting for the
  GPU, and a page never exits (a failed start shows its reason on the page).
- **Stack: 8 MiB** (wasm's default 1 MiB overflowed in the renderer).

## Single-player on one thread (how it fits together)
- The window's frame (`web_frame::run`): connection tasks and network turns,
  the server's step, then chunk jobs, in about 8 ms.
- The window joins its server through an in-memory pipe. Both ends' protocol
  code is `async` (`recv_async`); the browser polls them as futures, the
  desktop runs them on threads with `block_on`. Tested on one thread:
  `a_memory_login_runs_on_one_thread_when_both_ends_are_polled`.
- Waits: a sleep runs chunk jobs (`horizon_thread::set_idle`), the chunk
  results queue runs jobs while it waits, finite jobs run inline
  (`spawn_bounded`), and named worker threads fail so their callers' fallbacks
  run. `std::process::id`, std's condvars, sleeps and `recv_timeout` panic on
  wasm32 and are kept off the single-player path.
- Game files: the page reads the zip's table of contents and hands over only
  the files the game uses, still packed (`zip_reader.js`, `add_packed`).
- Kept between visits: what the game writes (worlds, options, its home), as one
  block in IndexedDB.

## Known limits and next steps
- A world's first opening runs in one go and freezes the page until its spawn
  area is made: make `prepare_levels`/`settle_spawn` steps of the frame.
- No sound (a WebAudio mixer for `audio.rs`), no screenshots, no multiplayer
  (WebSocket relay), no shader packs.
- Not yet tested with real game files or in WebKit (the iPad's engine).
- Size: 18.5 MiB page, 36 MiB wasm; leaving out unused features (mods,
  telemetry, realms) would shrink it.

## Milestones
1. ✅ `cargo check -p horizon-client --target wasm32-unknown-unknown` passes (cfg-gates above).
2. ✅ Renders through WebGL2 in a page: the "no game files" screen, then the
   title menu with a stand-in zip.
3. ⏳ Loads the player's game files (picker, IndexedDB, zip mount: done) and
   renders a world. Written and checked as far as possible without game files:
   the server runs in the window's frames, the in-memory connection's login runs
   as futures on one thread (tested), chunk work is pumped, and every wait on the
   single-player path was audited (no condvar, no sleeping loop that waits for a
   worker). Needs a real zip to test.
4. Playable single-player with input through the runner.
5. Bundled into one `.html` and tested in the runner; measure RAM.
6. Worlds and options kept in IndexedDB: ✅ (when the runner allows storage).
7. Later: WebAudio sound, WebSocket multiplayer, opening a world in steps (it
   freezes the page until its spawn area is made), a smaller wasm.
