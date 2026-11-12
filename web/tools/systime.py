#!/usr/bin/env python3
"""Keep std's SystemTime (file times are std's) and send `SystemTime::now()`
through horizon_clock::system_now, which also works in the browser build.

    python3 web/tools/systime.py CRATE_DIR...
"""
import os
import re
import sys

STD_ONLY = ("SystemTime", "UNIX_EPOCH", "SystemTimeError")


def split_use(m):
    items = [x.strip() for x in m.group(2).split(",") if x.strip()]
    web = [x for x in items if x.split(" as ")[0] not in STD_ONLY]
    std = [x for x in items if x.split(" as ")[0] in STD_ONLY]
    if not std:
        return m.group(0)
    indent = m.group(1)
    out = []
    if web:
        out.append(indent + ("use web_time::" + web[0] + ";" if len(web) == 1 else "use web_time::{" + ", ".join(web) + "};"))
    out.append(indent + ("use std::time::" + std[0] + ";" if len(std) == 1 else "use std::time::{" + ", ".join(std) + "};"))
    return "\n".join(out)


def fix(src):
    src = re.sub(r"^([ \t]*)use web_time::\{([^}]*)\};", split_use, src, flags=re.M)
    src = re.sub(r"\bweb_time::(SystemTime|UNIX_EPOCH|SystemTimeError)\b", r"std::time::\1", src)
    src = re.sub(r"(?<![\w:])(?:(?:std::time|web_time|time)::)?SystemTime::now\(\)", "horizon_clock::system_now()", src)
    return src


def add_dep(crate):
    path = os.path.join(crate, "Cargo.toml")
    text = open(path).read()
    if re.search(r"^horizon-clock\s*=", text, re.M):
        return
    rel = os.path.relpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "clock"), crate)
    text = re.sub(r"^\[dependencies\]\n", f"[dependencies]\n# SystemTime::now, also in the browser build (web/).\nhorizon-clock = {{ path = \"{rel}\" }}\n", text, count=1, flags=re.M)
    open(path, "w").write(text)


for crate in sys.argv[1:]:
    uses_now = False
    for sub in ("src", "tests", "examples", "benches"):
        for root, _, files in os.walk(os.path.join(crate, sub)):
            for f in files:
                if not f.endswith(".rs"):
                    continue
                p = os.path.join(root, f)
                src = open(p).read()
                new = fix(src)
                if new != src:
                    open(p, "w").write(new)
                if "horizon_clock::" in new:
                    uses_now = True
    if uses_now:
        add_dep(crate)
        print("clock", crate)
