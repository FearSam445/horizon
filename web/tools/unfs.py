#!/usr/bin/env python3
"""Puts back `.exists()`-style calls that webfs.py rewrote on a receiver that
isn't a path or a std::fs type: reads cargo's "no method named `fs_x`" errors
from a log and renames the call at each reported spot.

    python3 web/tools/unfs.py cargo.log
"""
import re
import sys

ERR = re.compile(r"error\[E0599\]: no method named `fs_(\w+)` found[^\n]*\n\s*--> ([^:\n]+):(\d+):(\d+)")
log = open(sys.argv[1]).read()
fixed = 0
for name, path, line, col in sorted(set(ERR.findall(log))):
    path = path.replace("crates/horizon-client-lib/../", "crates/")
    lines = open(path).read().split("\n")
    i, c = int(line) - 1, int(col) - 1
    if lines[i][c:].startswith("fs_" + name):
        lines[i] = lines[i][:c] + lines[i][c + 3:]
        open(path, "w").write("\n".join(lines))
        fixed += 1
print(fixed, "calls put back")
