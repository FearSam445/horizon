# Horizon in the browser

The browser build packs Horizon into **one HTML file**. It boots when opened
directly, or when dropped into an Eaglercraft-style runner that reads it as text
and `document.write()`s it. The plan and its status are in
[../WEB_PORT_PLAN.md](../WEB_PORT_PLAN.md).

## Build

```bash
python3 web/build.py --dev   # quick: profile web-dev, no wasm-opt
python3 web/build.py         # the shipped file: profile web (opt-level z, fat LTO), wasm-opt -Oz
```

Output: `web/out/horizon.html`. Needs the `wasm32-unknown-unknown` target,
`wasm-bindgen-cli` at the version in `Cargo.lock` (0.2.129) and, for the shipped
file, a current `wasm-opt` (binaryen 116 or newer; `WASM_OPT=path` picks one, e.g. from `npm install binaryen`).

Check it boots in headless Chromium (prints the console, saves a screenshot):

```bash
node web/test/boot.js 20 web/out/boot.png
```

## Layout

| Path | What |
|---|---|
| `horizon-web/` | The wasm entry point: sets the panic hook and runs `horizon_client::entry::main` |
| `page/` | The HTML page, its loader (unpacks the gzip + base64 game) and the input bridge |
| `fs/` (`horizon-fs`) | `std::fs` on the desktop; in the browser an in-memory file system with read-only zip mounts |
| `clock/` (`horizon-clock`) | `SystemTime::now()` that works in the browser |
| `shims/` | Stand-ins, under the original names, for crates that can't run in a page (mimalloc, ureq, mio, fs4, arboard) |
| `tools/` | The scripts that made the mechanical changes across the crates (`webdep.py`, `webtime.py`, `systime.py`, `webfs.py`) |
| `test/` | The headless boot check |

## How the desktop build stays the same

Every change is either behind `cfg(target_arch = "wasm32")` or goes through a
crate that is `std` itself on the desktop: `web_time` re-exports `std::time`,
`horizon_fs` re-exports `std::fs`, and `horizon_clock::system_now()` is
`SystemTime::now()`.

## Runner input

Runners and their controller scripts send synthetic `keydown`/`keyup` events to
`window` and mouse events to `canvas._eaglercraftX_canvas_element`. The game
draws on that canvas, and `page/input_bridge.js` passes the synthetic events on
as the keyboard and pointer events winit listens for. Real input is untouched.

## Game files

Minecraft's textures, sounds and data are not in the file and must never be
committed (see the repository's CLAUDE.md). The page asks the player for their
own prepared files and mounts them read-only in memory.
