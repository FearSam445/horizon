# Horizon in the browser

The browser build packs Horizon into **one HTML file**. It boots when opened
directly, or when dropped into an Eaglercraft-style runner that reads it as text
and `document.write()`s it. The plan and its status are in
[../WEB_PORT_PLAN.md](../WEB_PORT_PLAN.md).

## Playing it

1. On a computer, let the Horizon launcher prepare the game files from your own
   Minecraft: Java Edition (it does on first start). They are in its
   `game-files/horizon-client/26.3` folder (on Windows,
   `%APPDATA%\horizon-launcher\versions\<version>\.local\horizon-client\26.3`).
2. Zip that `26.3` folder (on an iPad: long-press it in the Files app, then
   Compress). Put the zip where the device can reach it (iCloud Drive,
   AirDrop...). The page reads only the files the game uses from it: not
   `horizon-data.bin`, which it never needs, nor the sounds until they play.
3. Open `horizon.html`, or drop it into an Eaglercraft-style runner, and choose
   the zip when it asks.

The zip, your worlds and your options are kept in the browser's storage when the
page may use it; inside some runners it may not, and then the zip is asked for
again and worlds last until the page closes.

### What to expect

- One thread does everything: the world, the chunks and the drawing take turns
  in each frame. A new world's spawn area takes a few seconds on a computer.
- The pause key is ~ (the ` key): it pauses and closes menus, as Escape does elsewhere.
  The keyboard's own Escape key does nothing (a runner's controller still pauses).
- Sound plays through Web Audio once you tap or press a key (browsers start a
  page muted). Each sound is read from your zip the first time it plays, so
  keep the zip where the page found it. Music and records don't play: a track
  takes tens of megabytes decoded.
- Only English and your device's language are loaded (the Language menu lists
  those).
- No screenshots, no multiplayer (a page can't open sockets), no shader packs
  or far terrain.
- Defaults are set for little memory: render distance 6, simulation distance 5.
  If it runs slowly, lower Resolution Scale in Horizon's graphics settings.
- Fullscreen does nothing: the page stays the size the runner gives it.

## Build

```bash
python3 web/build.py --dev   # quick: profile web-dev, no wasm-opt
python3 web/build.py         # the shipped file: profile web (opt-level 2, fat LTO), wasm-opt -O2: speed over size
```

Output: `web/out/horizon.html`. Needs the `wasm32-unknown-unknown` target,
`wasm-bindgen-cli` at the version in `Cargo.lock` (0.2.129) and, for the shipped
file, a current `wasm-opt` (binaryen 116 or newer; `WASM_OPT=path` picks one, e.g. from `npm install binaryen`).

Check it boots in headless Chromium (prints the console, saves a screenshot):

```bash
node web/test/boot.js 20 web/out/boot.png
```

Drive it through the menus with a real zip (`npm install playwright` first):
`web/test/drive.js` takes a list of steps (waits, clicks, keys, mouse moves,
screenshots, JavaScript to print); `GPU=1` draws with the graphics card, as a
player's browser does (the software renderer is slow and draws leaves
differently).

```bash
GPU=1 node web/test/drive.js path/to/26.3.zip steps.json
```

The status line every 2 s (`Horizon web: … turns, … memory N MiB`) gives the
frames run and the page's memory; each time the memory grows past another
64 MB the console shows the stack that grew it.

## Layout

| Path | What |
|---|---|
| `horizon-web/` | The wasm entry point: sets the panic hook and runs `horizon_client::entry::main` |
| `page/` | The HTML page, its loader (unpacks the gzip + base64 game) and the input bridge |
| `fs/` (`horizon-fs`) | `std::fs` on the desktop; in the browser an in-memory file system with read-only zip mounts, exported to the page's storage between visits |
| `thread/` (`horizon-thread`) | `std::thread` on the desktop; in the browser spawning runs inline, `spawn_bounded` runs finite work inline, `spawn_local` runs futures the page's loop polls, and a sleep does idle work |
| `clock/` (`horizon-clock`) | `SystemTime::now()` that works in the browser |
| `shims/` | Stand-ins, under the original names, for crates that can't run in a page (mimalloc, ureq, mio, fs4, arboard) |
| `tools/` | The scripts that made the mechanical changes across the crates (`webdep.py`, `webtime.py`, `systime.py`, `webfs.py`, `webthread.py`), and `uniforms.py`, which checks every WGSL uniform for WebGL2's 16-byte rule |
| `test/` | The headless boot check |

## How the desktop build stays the same

Every change is either behind `cfg(target_arch = "wasm32")` or goes through a
crate that is `std` itself on the desktop: `web_time` re-exports `std::time`,
`horizon_fs` re-exports `std::fs`, `horizon_thread` re-exports `std::thread`,
and `horizon_clock::system_now()` is `SystemTime::now()`. The connection code is
`async`: the desktop threads run it with `pollster::block_on`, as before.

## How the browser build runs on one thread

Each turn of the page's loop (`horizon_client::web_frame::run`, about 8 ms):
the connections' tasks (the client's and the server's login, as futures) and
the network's turn (`lan::io_pool`), the single-player server's step
(`Integrated::step`: a tick when due), then the job pool's chunk work
(`horizon_threads::jobs::pump`). Nothing may wait: std's condvars and sleeps
panic on wasm32, and WebGL's fences only signal between tasks.

## Runner input

Runners and their controller scripts send synthetic `keydown`/`keyup` events to
`window` and mouse events to `canvas._eaglercraftX_canvas_element`. The game
draws on that canvas, and `page/input_bridge.js` passes the synthetic events on
as the keyboard and pointer events winit listens for. Real input is untouched.

## Game files

Minecraft's textures, sounds and data are not in the file and must never be
committed (see the repository's CLAUDE.md). The page asks the player for their
own prepared files and mounts them read-only in memory.
