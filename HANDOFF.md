# Handoff: Horizon in one HTML file (browser / iPad)

Read this first in a new session. It says what the goal is, what works, what is
left, how the browser build is put together, and how to build and test it on a
Windows PC. The design notes are in `WEB_PORT_PLAN.md` and `web/README.md`.

## The goal

The owner wants **one `.html` file** of Horizon (Minecraft 26.3 rewritten in Rust,
this repository) that boots when dropped into their **HTML runner**
(`web/runner/WASM_Runner_Controller.html`, a CodeHS web app). It must use **as little
RAM as possible**. The target device is an **iPad 10th gen** (Safari/WebKit, iOS 27)
with keyboard and mouse, the runner running inside CodeHS's sandboxed frame. The
launcher is out of scope. Minecraft's own files are never put in the HTML: the player
picks a zip of their prepared `26.3` folder, which the launcher makes on their PC at
`%APPDATA%\horizon-launcher\versions\<version>\.local\horizon-client\26.3`.

The owner is not a programmer: explain results simply, do everything yourself
(installs, builds, tests, commits), and only ask when something truly needs them.

All work is on branch **`web-port`** of the owner's fork **FearSam445/horizon**
(upstream: horizon-rust/horizon). Commit there; never commit Mojang files or the zip.
**Pushing needs the owner's GitHub sign-in** (no credentials on the PC as of
Nov 13): commits made since `eb320fb63e` may be local only. Check with
`git status -sb`; the owner pushes with `git push origin web-port`.

## Status (Nov 13, tested in desktop Chromium with the owner's real zip)

Works:
- The page boots, reads the zip, shows the title screen and menus, and **creates
  and opens a world**; the player walks, jumps, looks around (60 fps on the PC's
  GPU). The canvas fills the page (1180x820, the iPad's landscape size, tested).
- **Sound** through Web Audio (cpal's web host): effects play, read from the zip
  as each first plays. Music and records don't (a decoded track is tens of MB).
- **Memory** (the page's wasm memory, which never shrinks): title 407 MB, a new
  world about 490 MB (was 708 MB at the title before Nov 13).

Not yet confirmed on the iPad itself (WebKit): the fixes were tested in Chromium.
The owner tests on the iPad; ask what they saw.

Fixed on Nov 13 (see `git log`):
1. Worlds never finished loading: the bar stopped at ~17% (10 of 59 parts: the
   server prepared, no spawn chunk counted). The spawn area was ready in 2 s;
   then the page stopped on two panics: the region writer thread
   (`save.rs`, now writes inline without a thread) and WebGL2's missing base
   vertex (the vendored wgpu-hal now binds vertex buffers from it).
2. The picture didn't fit the iPad: winit wrote the game's desktop window size
   on the canvas as inline pixels; the page's CSS now wins (`!important`).
3. The runner's synthetic mouse never turned the view (winit reads motion from
   `getCoalescedEvents()`, empty for script-made events): the bridge fills it.
4. Sound, memory (see below), the language files (only English and the
   device's language are loaded: 17 MB less), no world-icon grabs or audio
   device scans each second.

## Ideas for what's next, in order of value

1. **iPad feedback**: anything that differs in Safari (input, sound unlock,
   sizing in the runner, crashes). Safari's console: on a Mac, Develop menu.
2. **Speed on the iPad**: the shipped build uses `opt-level` and `wasm-opt`
   settings chosen on Nov 13 (see "Building"); compare frames (`turns` in the
   status line) if changed. Lower render distance helps most.
3. **Memory**: the remaining peaks, from the "memory now" lines: the HUD atlas
   (96 MB of pixels while it is built; 4096x5120), the block sprites' mip chain
   (`gfx::mip_chain`, ~66 MB), `Sprites::lay_out`, facts.json read whole
   (77 MB) by `BlockFacts::load_with_projection`. Reading JSON through a
   reader and smaller atlases would cut more.
4. Not done: music (would need streaming decode), screenshots, multiplayer
   (no sockets in a page; a WebSocket relay), shader packs, a smaller wasm.

## How the browser build works

Target `wasm32-unknown-unknown` (winit and wasm-bindgen need it). One thread: a
runner's sandbox has no SharedArrayBuffer. Everything std can't do there goes
through small crates in `web/` that are std itself on the desktop:

| Thing | Desktop | Browser |
|---|---|---|
| `web_time` | `std::time` | the page's clock (std's `Instant::now` panics) |
| `horizon_clock::system_now()` | `SystemTime::now()` | `Date.now` |
| `horizon_fs` (`web/fs`) | `std::fs` | in-memory file system; game files added packed (`add_packed`), inflated on open; `export`/`import` keep worlds in IndexedDB; `fetch`/`withhold` for files the page hands over later (sounds) |
| `horizon_fs::process_id()` | `std::process::id()` | 1 (std's panics) |
| `horizon_thread` (`web/thread`) | `std::thread` | `spawn`/scoped run inline; `Builder::spawn` fails (callers fall back); `spawn_bounded` runs finite work inline; `spawn_local`/`run_local_tasks` poll futures; `sleep` runs idle work (`set_idle` = chunk jobs) |
| `web/shims/*` | the real crates | stand-ins under the same names: mimalloc, ureq, mio, fs4, arboard |

**Any `Builder::spawn(...).expect(...)` panics in the browser**: give it an
inline fallback (as `save.rs`'s `start_writer` and `audio.rs` do).

Each turn of the page's loop, `horizon_client::web_frame::run` (called from
`about_to_wait`) runs, in about 8 ms: the connection tasks and `lan::io_pool`
turns, the single-player server's step (`Integrated::step`: while settling, 40 ms
of spawn work; then a tick when due), then chunk jobs. Every 2 s it logs
`Horizon web: N turns, … memory M MiB`.

Single player: the window joins its integrated server through an in-memory pipe;
the login is `async` on both ends (`ConnReader::recv_async`).

Rules that bite on wasm32: std's condvar waits, `thread::sleep`, `recv_timeout`,
`Instant::now` and `process::id` panic; `Builder::spawn` fails; WebGL fences only
signal between tasks, so nothing may block waiting for the GPU, and staging
buffers are freed a task later (big uploads: `gfx::direct_upload`); WebGL2 has no
base vertex (emulated in `third_party/wgpu-hal/src/gles/command.rs`); a page never
exits (`logging::show_on_page` shows failures); its memory never shrinks, so
every peak counts. The wasm stack is 8 MiB (`.cargo/config.toml`).

The page (`web/page/`): `index.html` (canvas with class
`_eaglercraftX_canvas_element`, which the runner's controller script targets;
the audio unlock, which must run before the glue), `loader.js` (unpacks the
gzip+base64 wasm, asks for the zip, names the withheld sounds, answers the
game's file requests, keeps worlds), `zip_reader.js` (hands over only the files
the game uses; `withheld` reads one entry on demand), `input_bridge.js` (the
runner's synthetic mouse/key events to what winit listens for; backquote works
as Escape).

Sound: `audio.rs` on wasm decodes a static sound on the game's thread once
`horizon_fs::fetch` says its file is here (`pump_waiting` each tick), then drops
the file; the page polls `wanted_files()` every 50 ms. The sound registry lists
withheld files through `horizon_fs::withheld_under`.

## Building on Windows

Installed on the owner's PC (Nov 13): Git, Rust 1.97+ with
`wasm32-unknown-unknown`, Python 3.14, Node 26, `wasm-bindgen-cli` 0.2.129,
binaryen through npm (`%APPDATA%\npm\wasm-opt.cmd`, version 132), Playwright
with its Chromium (in `~/horizon/node_modules`, kept out of git through
`.git/info/exclude`). The repo is at `%USERPROFILE%\horizon`; a second checkout
`%USERPROFILE%\horizon-ship` (git worktree) builds the shipped file so its
long build isn't disturbed by edits.

```powershell
python web\build.py --dev      # quick build (2-3 min): web\out\horizon.html
set WASM_OPT=%APPDATA%\npm\wasm-opt.cmd
python web\build.py            # the shipped file: profile `web` (fat LTO), wasm-opt (about 15 min here)
```

`HORIZON_WASM_OPT` picks wasm-opt's level (default `-O2`; `-Oz` with `CARGO_PROFILE_WEB_OPT_LEVEL=z` is 25% smaller and half as fast); cargo's
`CARGO_PROFILE_WEB_OPT_LEVEL` overrides the profile's. Never edit the sources
while a build of the same checkout runs: it compiles what it finds.

Check the desktop build still works after changes:
`cargo check --workspace --all-targets` (no warnings) and for the browser
`cargo check -p horizon-web --target wasm32-unknown-unknown`. Tests that matter:
`cargo test -p horizon-client-lib --lib -- net:: lan::`, `cargo test -p horizon-fs`,
`cargo test -p horizon-client-render --lib -- audio sound_events hud`.
Commits run `tools/check_public_tree.py` (`git config core.hooksPath .githooks`).

## Testing

The owner's zip for tests: `C:\Users\samma\OneDrive\Documents\mconipadreal\26.3.zip`
(447 MB, outside the repo).

- `web/test/drive.js` drives the page through steps (see `web/README.md`).
  With `GPU=1` it uses the PC's graphics card (an RTX 4060 Ti; Chromium's
  software renderer is slow and draws leaves differently). Coordinates at the
  default 1180x820: welcome screen's Continue (590,770), title's Singleplayer
  (590,377), Create New World (355,770), then click the world (590,410) to
  capture the mouse. `{"eval": "window.horizonServed"}` counts the sounds read
  from the zip, `window.horizonAudioContexts` the audio contexts.
- `RUNNER=1` opens the page through `web/runner/WASM_Runner_Controller.html`
  (picks the game file in it, as the owner does); `OUT=folder` tests another
  build's `horizon.html` (e.g. `%USERPROFILE%\horizon-ship\web\out`). Checked
  Nov 13 through the runner: world, sounds, the runner's pointer-lock shim.
- `web/test/boot.js` boots headless and prints the console.
- On the iPad: drop `horizon.html` into the runner. Safari is WebKit; Chrome on
  the PC is not.

## The runner

`web/runner/WASM_Runner_Controller.html` is the owner's runner, kept CodeHS-friendly:
it reads the picked `.html` as text and `document.write`s it with a pointer-lock shim
and a controller script injected. Additions: it remembers the last game file in
IndexedDB ("Play again"), has iPad web-app meta tags, and blocks zoom. Keep it one
plain HTML file.
