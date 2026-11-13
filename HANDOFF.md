# Handoff: Horizon in one HTML file (browser / iPad)

Read this first in a new session. It says what the goal is, what works, what is
broken right now, how the browser build is put together, and how to build and test
it on a Windows PC. The design notes are in `WEB_PORT_PLAN.md` and `web/README.md`.

## The goal

The owner wants **one `.html` file** of Horizon (Minecraft 26.3 rewritten in Rust,
this repository) that boots when dropped into their **HTML runner**
(`web/runner/WASM_Runner_Controller.html`, a CodeHS web app). It must use **as little
RAM as possible**. The target device is an **iPad 10th gen** (Safari/WebKit, iOS 27)
with keyboard and mouse, the runner running inside CodeHS's sandboxed frame. The
launcher is out of scope. Minecraft's own files are never put in the HTML: the player
picks a zip of their prepared `26.3` folder, which the launcher makes on their PC at
`%APPDATA%\horizon-launcher\versions\<version>\.local\horizon-client\26.3`.

All work is on branch **`web-port`** of the owner's fork **FearSam445/horizon**
(upstream: horizon-rust/horizon). Commit and push there; never commit Mojang files.

## Status on the iPad (last test, Nov 12)

Works: the page boots, the zip is read, the title screen (panorama, menus, Horizon's
settings screens, the first-run accessibility screen) renders through WebGL2.

**Broken now, in order of importance:**

1. **Worlds never finish loading.** Creating a world reaches "Loading terrain..." and
   the bar stops at about 20%. The page doesn't freeze any more (the spawn area is
   made in steps, `Integrated::step` with `settle_spawn_step`), but it never
   completes. Not yet diagnosed. Ideas to check first, with real game files on the PC
   (desktop Chrome, see Testing):
   - is chunk work running at all (`horizon_threads::jobs::pump` from
     `web_frame::run`, `Integrated::step`, and `horizon_thread::sleep` via
     `set_idle`)? Log jobs run per frame.
   - does the client's login over the in-memory pipe complete (`net.rs` `run` as a
     `spawn_local` task, `lan/conn.rs` `serve_local` task, `lan::io_pool_pump`)?
     Log the login steps on both sides.
   - is the 20% the server's own spawn load (`flags.load` / `LevelLoadingScreen`)
     or the client waiting for chunks it never gets (`io_pool` play-state pump)?
   - anything still waiting on a thread that never started (a `Builder::spawn`
     that fails on wasm and whose fallback is missing), or a `try_borrow` in
     `Integrated` that skips work every frame.
2. **The frame doesn't fit the screen.** There is a black bar at the bottom and the
   picture runs off the right edge (the welcome screen's text is cut off). An earlier
   fake `devicePixelRatio = 1` made it worse and was removed. Look at how winit sizes
   the canvas on web (`web_canvas` in `crates/horizon-client-input/src/window.rs`,
   the canvas CSS in `web/page/index.html`: `100vw`/`100vh`; iPad Safari's `100vh`
   includes the toolbars, try `100dvh`/`position: fixed; inset: 0`), and whether the
   game's surface size follows the canvas's resize (winit `Resized` on web).
3. Fullscreen: made a no-op on wasm (it broke the page in the runner).

Not done yet: sound (WebAudio), screenshots, multiplayer (no sockets in a page),
shader packs, a smaller wasm.

## How the browser build works

Target `wasm32-unknown-unknown` (winit and wasm-bindgen need it). One thread: a
runner's sandbox has no SharedArrayBuffer. Everything std can't do there goes
through small crates in `web/` that are std itself on the desktop:

| Thing | Desktop | Browser |
|---|---|---|
| `web_time` | `std::time` | the page's clock (std's `Instant::now` panics) |
| `horizon_clock::system_now()` | `SystemTime::now()` | `Date.now` |
| `horizon_fs` (`web/fs`) | `std::fs` | in-memory file system; game files added packed (`add_packed`), inflated on open; `export`/`import` keep worlds in IndexedDB |
| `horizon_fs::process_id()` | `std::process::id()` | 1 (std's panics) |
| `horizon_thread` (`web/thread`) | `std::thread` | `spawn`/scoped run inline; `Builder::spawn` fails (callers fall back); `spawn_bounded` runs finite work inline; `spawn_local`/`run_local_tasks` poll futures; `sleep` runs idle work (`set_idle` = chunk jobs) |
| `web/shims/*` | the real crates | stand-ins under the same names: mimalloc, ureq, mio, fs4, arboard |

Mechanical rewrites were made by the scripts in `web/tools/` (webtime, systime, webfs,
webthread, webdep, fixdeps, unfs). `web/tools/uniforms.py` checks WGSL uniforms for
WebGL2's 16-byte rule.

Each turn of the page's loop, `horizon_client::web_frame::run` (called from
`about_to_wait`) runs, in about 8 ms: the connection tasks and `lan::io_pool` turns,
the single-player server's step (`Integrated::step`: while settling, 40 ms of spawn
work; then a tick when due), then chunk jobs.

Single player: the window joins its integrated server through an in-memory pipe. The
login code on both ends is `async` (`ConnReader::recv_async`); desktop threads run it
with `pollster::block_on` unchanged, the browser polls it as futures. Test:
`lan::conn::tests::a_memory_login_runs_on_one_thread_when_both_ends_are_polled`.

Rules that bite on wasm32: std's condvar waits, `thread::sleep`, `recv_timeout`,
`Instant::now` and `process::id` panic; `Builder::spawn` fails; WebGL fences only
signal between tasks, so nothing may block waiting for the GPU; a page never exits
(`logging::show_on_page` shows failures). The wasm stack is 8 MiB
(`.cargo/config.toml`).

The page (`web/page/`): `index.html` (canvas with class
`_eaglercraftX_canvas_element`, which the runner's controller script targets),
`loader.js` (unpacks the gzip+base64 wasm, asks for the zip, keeps worlds),
`zip_reader.js` (hands over only the files the game uses: not `horizon-data.bin`,
sounds, macOS extras), `input_bridge.js` (the runner's synthetic mouse/key events
to what winit listens for; backquote works as Escape).

## Building on Windows

Install: Rust (rustup, toolchain 1.97 or newer), Python 3, Node.js 22, Git.

```powershell
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --locked --version 0.2.129
npm install -g binaryen        # wasm-opt 116+; Ubuntu's 108 writes modules browsers refuse
git clone https://github.com/FearSam445/horizon
cd horizon
git checkout web-port
python web/build.py --dev      # quick build: web/out/horizon.html
```

`wasm-bindgen-cli` must match the `wasm-bindgen` version in `Cargo.lock` (0.2.129).
For the shipped file: `set WASM_OPT=<path to wasm-opt>` then `python web/build.py`
(profile `web`: opt-level z, fat LTO; about 30 minutes). `HORIZON_WEB_LOG=debug`
(or `trace`) at build time adds the renderer's debug lines to the console.

Check the desktop build still works after changes:
`cargo check --workspace --all-targets` (no warnings) and for the browser
`cargo check -p horizon-web --target wasm32-unknown-unknown`. Tests that matter:
`cargo test -p horizon-client-lib --lib -- net:: lan::`, `cargo test -p horizon-fs`.

## Testing

On the PC, the owner's real prepared folder is available: zip
`...\.local\horizon-client\26.3`, then:

- In desktop Chrome: open `web/out/horizon.html`, choose the zip, open DevTools'
  console. This is the fastest way to debug the world-loading problem with real data.
- Headless: `npm install playwright` then
  `node web/test/boot.js 60 web/out/boot.png path\to\26.3.zip` prints the console,
  pauses a busy page to print its stack, and saves a screenshot.
- On the iPad: drop `horizon.html` into the runner. Safari is WebKit; Chrome on the
  PC is not, so check fixes there too.

## The runner

`web/runner/WASM_Runner_Controller.html` is the owner's runner, kept CodeHS-friendly:
it reads the picked `.html` as text and `document.write`s it with a pointer-lock shim
and a controller script injected. Additions: it remembers the last game file in
IndexedDB ("Play again"), has iPad web-app meta tags, and blocks zoom. Keep it one
plain HTML file.
