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
  threads. Long-lived threads become steps the main loop calls each frame (the
  integrated server first); short ones run inline.

## Milestones
1. ✅ `cargo check -p horizon-client --target wasm32-unknown-unknown` passes (cfg-gates above).
2. Renders a blank WebGL2 canvas from a test page.
3. Loads the player's game files and renders a world.
4. Playable single-player with input through the runner.
5. Bundled into one `.html` and tested in the runner; measure RAM.
6. Later: saves to IndexedDB, WebAudio sound, WebSocket multiplayer.
