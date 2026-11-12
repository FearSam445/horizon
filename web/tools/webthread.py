#!/usr/bin/env python3
"""Move crates from std::thread to horizon_thread (web/thread): std::thread on
the desktop, inline spawning in the browser build's single thread.

    python3 web/tools/webthread.py CRATE_DIR...

Rewrites `std::thread::X`, `use std::thread;` and `thread` inside
`use std::{...}` groups, skips comment lines, and adds `horizon-thread` to
each changed crate. `std::thread_local!` is left alone.
"""
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from webtime import split_top  # noqa: E402

GROUP = re.compile(r"use std::\{")


def fix_groups(src):
    out, i = [], 0
    for m in GROUP.finditer(src):
        if m.start() < i:
            continue
        depth, j = 1, m.end()
        while depth:
            depth += {"{": 1, "}": -1}.get(src[j], 0)
            j += 1
        if src[j] != ";":
            continue
        items = split_top(src[m.end(): j - 1])
        moved = [it for it in items if re.match(r"\s*thread(::|\s*$)", it)]
        if not moved:
            continue
        rest = [it for it in items if it not in moved]
        line_start = src.rfind("\n", 0, m.start()) + 1
        indent = src[line_start: m.start()]
        news = []
        for it in moved:
            it = it.strip()
            if it == "thread":
                news.append("use horizon_thread as thread;")
            else:
                inner = re.sub(r"\bself\b(?!\s+as)", "self as thread", it[len("thread::"):].strip())
                news.append("use horizon_thread::" + inner + ";")
        if rest:
            if len(rest) == 1 and "{" not in rest[0]:
                head = f"use std::{rest[0].strip()};"
            else:
                head = "use std::{" + ", ".join(x.strip() for x in rest) + "};"
            replacement = head + "".join("\n" + indent + n for n in news)
        else:
            replacement = ("\n" + indent).join(news)
        out.append(src[i: m.start()])
        out.append(replacement)
        i = j + 1
    out.append(src[i:])
    return "".join(out)


def fix(src):
    lines = []
    for line in src.split("\n"):
        if line.lstrip().startswith("//"):
            lines.append(line)
            continue
        line = re.sub(r"\buse std::thread;", "use horizon_thread as thread;", line)
        line = re.sub(r"(?<![\w:])(::)?std::thread::", "horizon_thread::", line)
        line = re.sub(r"(use horizon_thread::\{)([^}]*)\}",
                      lambda m: m.group(1) + re.sub(r"\bself\b(?!\s+as)", "self as thread", m.group(2)) + "}", line)
        lines.append(line)
    return fix_groups("\n".join(lines))


def add_dep(crate):
    path = os.path.join(crate, "Cargo.toml")
    text = open(path).read()
    if re.search(r"^horizon-thread\s*=", text, re.M):
        return
    rel = os.path.relpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "thread"), crate)
    line = f"# std::thread, inline in the browser build's one thread (web/).\nhorizon-thread = {{ path = \"{rel}\" }}\n"
    if re.search(r"^\[dependencies\]\s*$", text, re.M):
        text = re.sub(r"^\[dependencies\]\s*\n", "[dependencies]\n" + line, text, count=1, flags=re.M)
    else:
        m = re.search(r"^\[(?!package\])", text, re.M)
        block = "[dependencies]\n" + line + "\n"
        text = text[: m.start()] + block + text[m.start():] if m else text.rstrip() + "\n\n" + block
    open(path, "w").write(text)


if __name__ == "__main__":
    for crate in sys.argv[1:]:
        changed = False
        for sub in ("src", "tests", "examples", "benches"):
            for root, _, files in os.walk(os.path.join(crate, sub)):
                for f in files:
                    if f.endswith(".rs"):
                        p = os.path.join(root, f)
                        src = open(p).read()
                        new = fix(src)
                        if new != src:
                            open(p, "w").write(new)
                            changed = True
        if changed:
            add_dep(crate)
            print("moved", crate)
