#!/usr/bin/env python3
"""Move crates from std::fs to horizon_fs (web/fs): std::fs itself on the
desktop, an in-memory file system in the browser build.

    python3 web/tools/webfs.py CRATE_DIR...

Rewrites `std::fs::X`, `use std::fs;`, `fs` inside `use std::{...}` groups and
the `Path` methods that read the file system (`.exists()` -> `.fs_exists()`,
with `horizon_fs::PathExt` imported), skips comment lines, and adds
`horizon-fs` to each changed crate. A non-Path receiver of a rewritten method
fails to compile, and is put back by hand.
"""
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from webtime import split_top  # noqa: E402

METHODS = ["exists", "try_exists", "is_file", "is_dir", "is_symlink", "metadata",
           "symlink_metadata", "canonicalize", "read_dir"]
GROUP = re.compile(r"use std::\{")
IMPORT = "use horizon_fs::{KindExt as _, MetaExt as _, PathExt as _};"


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
        inner = src[m.end(): j - 1]
        items = split_top(inner)
        fsed = [it for it in items if re.match(r"\s*fs(::|\s*$)", it)]
        if not fsed:
            continue
        rest = [it for it in items if it not in fsed]
        line_start = src.rfind("\n", 0, m.start()) + 1
        indent = src[line_start: m.start()]
        news = []
        for it in fsed:
            it = it.strip()
            news.append("use horizon_fs as fs;" if it == "fs" else "use horizon_fs::" + it[len("fs::"):].strip() + ";")
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


METHOD_RE = re.compile(r"\.(" + "|".join(METHODS) + r")\(\)")


def fix(src):
    lines, uses_ext = [], False
    for line in src.split("\n"):
        if line.lstrip().startswith("//"):
            lines.append(line)
            continue
        line = re.sub(r"\buse std::fs;", "use horizon_fs as fs;", line)
        line = re.sub(r"(?<![\w:])(::)?std::fs::", "horizon_fs::", line)
        new = METHOD_RE.sub(lambda m: ".fs_" + m.group(1) + "()", line)
        if new != line:
            uses_ext = True
        lines.append(new)
    src = fix_groups("\n".join(lines))
    if uses_ext and IMPORT not in src:
        src = add_import(src)
    return src


def add_import(src):
    """`use horizon_fs::PathExt as _;` after the file's header, at module level."""
    lines = src.split("\n")
    i = 0
    while i < len(lines) and (lines[i].startswith("//") or lines[i].startswith("#![") or lines[i].strip() == ""):
        i += 1
    lines.insert(i, IMPORT)
    return "\n".join(lines)


def add_dep(crate):
    path = os.path.join(crate, "Cargo.toml")
    text = open(path).read()
    if re.search(r"^horizon-fs\s*=", text, re.M):
        return
    rel = os.path.relpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "fs"), crate)
    text = re.sub(r"^\[dependencies\]\n", f"[dependencies]\n# std::fs, in memory in the browser build (web/).\nhorizon-fs = {{ path = \"{rel}\" }}\n", text, count=1, flags=re.M)
    open(path, "w").write(text)


if __name__ == "__main__":
    for crate in sys.argv[1:]:
        changed = False
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
                        changed = True
        if changed:
            add_dep(crate)
            print("moved", crate)
