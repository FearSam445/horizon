#!/usr/bin/env python3
"""Builds Horizon for the browser as one HTML file.

    python3 web/build.py            # the shipped file: profile `web`, wasm-opt -Oz
    python3 web/build.py --dev      # quicker: profile `web-dev`, no wasm-opt

Writes web/out/horizon.html. The page needs nothing else: the game is in it
(gzip + base64, unpacked by the browser's DecompressionStream), it draws on a
canvas Eaglercraft runners send their controller input to, and it can be
opened directly or dropped into a runner that document.write()s it.
"""
import argparse
import base64
import gzip
import os
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WEB = os.path.join(ROOT, "web")
OUT = os.path.join(WEB, "out")
TARGET = "wasm32-unknown-unknown"


def run(cmd, **kw):
    print("+", " ".join(cmd), flush=True)
    subprocess.run(cmd, check=True, **kw)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dev", action="store_true", help="quicker build, bigger file")
    ap.add_argument("--no-cargo", action="store_true", help="pack the last build")
    args = ap.parse_args()
    profile = "web-dev" if args.dev else "web"
    if not args.no_cargo:
        run(["cargo", "build", "-p", "horizon-web", "--target", TARGET, "--profile", profile], cwd=ROOT)
    wasm = os.path.join(ROOT, "target", TARGET, profile, "horizon_web.wasm")
    pkg = os.path.join(OUT, "pkg")
    shutil.rmtree(pkg, ignore_errors=True)
    run(["wasm-bindgen", "--target", "no-modules", "--no-typescript", "--out-dir", pkg, wasm])
    bg = os.path.join(pkg, "horizon_web_bg.wasm")
    if not args.dev and shutil.which("wasm-opt"):
        run(["wasm-opt", "-Oz", "--all-features", "--strip-debug", "--strip-producers", bg, "-o", bg])
    glue = open(os.path.join(pkg, "horizon_web.js")).read()
    data = open(bg, "rb").read()
    packed = base64.b64encode(gzip.compress(data, compresslevel=9, mtime=0)).decode()
    page = open(os.path.join(WEB, "page", "index.html")).read()
    loader = open(os.path.join(WEB, "page", "loader.js")).read()
    bridge = open(os.path.join(WEB, "page", "input_bridge.js")).read()
    html = (
        page.replace("/*INPUT_BRIDGE*/", bridge)
        .replace("/*GLUE*/", glue.replace("</script", "<\\/script"))
        .replace("/*LOADER*/", loader)
        .replace("WASM_BASE64", packed)
    )
    os.makedirs(OUT, exist_ok=True)
    out = os.path.join(OUT, "horizon.html")
    open(out, "w").write(html)
    mb = lambda n: f"{n / 1048576:.1f} MiB"
    print(f"wasm {mb(len(data))}, gzip+base64 {mb(len(packed))}, page {mb(len(html))}: {out}")


if __name__ == "__main__":
    sys.exit(main())
