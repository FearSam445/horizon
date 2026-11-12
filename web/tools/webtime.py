#!/usr/bin/env python3
"""Move crates from std::time to web_time, which is std::time itself on the
desktop and the browser's clock on wasm32 (std's Instant::now panics there).

    python3 web/tools/webtime.py CRATE_DIR...

Rewrites `std::time::X`, `use std::time;` and `time::...` inside `use std::{...}`
groups, skips comment lines, and adds `web-time` to each changed crate.
"""
import os
import re
import sys

GROUP = re.compile(r"use std::\{")


def split_top(s):
    items, depth, cur = [], 0, ""
    for ch in s:
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
        if ch == "," and depth == 0:
            items.append(cur)
            cur = ""
        else:
            cur += ch
    if cur.strip():
        items.append(cur)
    return items


def fix_groups(src):
    out, i = [], 0
    for m in GROUP.finditer(src):
        if m.start() < i:
            continue
        # The group's closing brace and semicolon.
        depth, j = 1, m.end()
        while depth:
            depth += {"{": 1, "}": -1}.get(src[j], 0)
            j += 1
        if src[j] != ";":
            continue
        inner = src[m.end(): j - 1]
        items = split_top(inner)
        timed = [it for it in items if re.match(r"\s*time(::|\s*$)", it)]
        if not timed:
            continue
        rest = [it for it in items if it not in timed]
        line_start = src.rfind("\n", 0, m.start()) + 1
        indent = src[line_start: m.start()]
        news = []
        for it in timed:
            it = it.strip()
            if it == "time":
                news.append("use web_time as time;")
            else:
                news.append("use web_time::" + it[len("time::"):].strip() + ";")
        if rest:
            multiline = "\n" in inner
            body = ",".join(rest).strip().rstrip(",")
            if len(rest) == 1 and "{" not in body:
                head = f"use std::{body.strip()};"
            elif multiline:
                head = "use std::{" + ",".join(rest).rstrip().rstrip(",") + (",\n" + indent if inner.rstrip().endswith(",") else "") + "};"
                if not inner.rstrip().endswith(","):
                    head = "use std::{" + ",".join(rest) + "};"
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
        line = re.sub(r"\buse std::time;", "use web_time as time;", line)
        line = re.sub(r"(?<![\w:])(::)?std::time::", "web_time::", line)
        lines.append(line)
    return fix_groups("\n".join(lines))


def add_dep(crate):
    path = os.path.join(crate, "Cargo.toml")
    text = open(path).read()
    if re.search(r"^web-time\s*=", text, re.M):
        return
    text = re.sub(r"^\[dependencies\]\n", "[dependencies]\n# std::time, with a clock in the browser build (web/).\nweb-time = \"1.1\"\n", text, count=1, flags=re.M)
    open(path, "w").write(text)


if __name__ == "__main__":
    for crate in sys.argv[1:]:
        changed = False
        for root, _, files in os.walk(os.path.join(crate, "src")):
            for f in files:
                if not f.endswith(".rs"):
                    continue
                p = os.path.join(root, f)
                src = open(p).read()
                new = fix(src)
                if new != src:
                    open(p, "w").write(new)
                    changed = True
        for extra in ("tests", "examples", "benches"):
            d = os.path.join(crate, extra)
            for root, _, files in os.walk(d) if os.path.isdir(d) else []:
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
