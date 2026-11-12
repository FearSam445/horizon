#!/usr/bin/env python3
"""Adds web/'s crates (horizon-fs, horizon-clock, web-time) to every crate in
crates/ whose code uses them but whose manifest doesn't name them.

    python3 web/tools/fixdeps.py
"""
import os
import re
import subprocess

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
DEPS = {
    "horizon_fs": ("horizon-fs", '{ path = "../../web/fs" }', "std::fs, in memory in the browser build (web/)."),
    "horizon_clock": ("horizon-clock", '{ path = "../../web/clock" }', "SystemTime::now, also in the browser build (web/)."),
    "web_time": ("web-time", '"1.1"', "std::time, with a clock in the browser build (web/)."),
}


def sources(crate):
    """The crate's source folders: its own, and for horizon-client-lib horizon-client's."""
    text = open(os.path.join(crate, "Cargo.toml")).read()
    dirs = [os.path.join(crate, d) for d in ("src", "tests", "examples", "benches")]
    for m in re.finditer(r'^path\s*=\s*"([^"]+)"', text, re.M):
        dirs.append(os.path.dirname(os.path.normpath(os.path.join(crate, m.group(1)))))
    return [d for d in dirs if os.path.isdir(d)]


def uses(crate, name):
    for d in sources(crate):
        r = subprocess.run(["grep", "-rlw", "--include=*.rs", name, d], capture_output=True, text=True)
        if r.stdout.strip():
            return True
    return False


for entry in sorted(os.listdir(os.path.join(ROOT, "crates"))):
    crate = os.path.join(ROOT, "crates", entry)
    manifest = os.path.join(crate, "Cargo.toml")
    if not os.path.exists(manifest):
        continue
    text = open(manifest).read()
    for name, (dep, spec, why) in DEPS.items():
        if re.search(rf"^{re.escape(dep)}\s*=", text, re.M) or not uses(crate, name):
            continue
        line = f"# {why}\n{dep} = {spec}\n"
        if re.search(r"^\[dependencies\]\s*$", text, re.M):
            text = re.sub(r"^\[dependencies\]\s*\n", "[dependencies]\n" + line, text, count=1, flags=re.M)
        else:
            m = re.search(r"^\[(?!package\])", text, re.M)
            block = "[dependencies]\n" + line + "\n"
            text = text[: m.start()] + block + text[m.start():] if m else text.rstrip() + "\n\n" + block
        print(f"{entry}: + {dep}")
    open(manifest, "w").write(text)
