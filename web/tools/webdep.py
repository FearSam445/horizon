#!/usr/bin/env python3
"""Move a dependency of a crate behind `not(wasm32)` and, optionally, give the
browser build a stand-in from web/shims under the same name.

    python3 web/tools/webdep.py CRATE_DIR DEP [SHIM_DIR_NAME]

With a shim, the crate root gets `extern crate horizon_web_<x> as <dep>;`
for wasm32, so the crate's code keeps using the dependency's own name.
"""
import os
import re
import sys

NATIVE = "[target.'cfg(not(target_arch = \"wasm32\"))'.dependencies]"
WEB = "[target.'cfg(target_arch = \"wasm32\")'.dependencies]"


def section_add(text, header, line):
    if header in text:
        i = text.index(header) + len(header)
        return text[:i] + "\n" + line + text[i:]
    # Before [features], [lints], [dev-dependencies] or the end.
    m = re.search(r"^\[(features|lints|dev-dependencies|build-dependencies|\[)", text, re.M)
    block = header + "\n" + line + "\n\n"
    if m:
        return text[: m.start()] + block + text[m.start():]
    return text.rstrip("\n") + "\n\n" + block


def main():
    crate, dep = sys.argv[1], sys.argv[2]
    shim = sys.argv[3] if len(sys.argv) > 3 else None
    path = os.path.join(crate, "Cargo.toml")
    text = open(path).read()
    # The dependency's line in [dependencies] (a single line).
    deps = re.search(r"^\[dependencies\]\n(.*?)(?=^\[)", text, re.M | re.S)
    # The dependency's line, with the comment lines right above it.
    pat = re.compile(r"^(?:#.*\n)*" + re.escape(dep) + r"\s*=.*\n", re.M)
    m = pat.search(deps.group(1)) if deps else None
    if m:
        line = m.group(0).rstrip("\n")
        start = deps.start(1) + m.start()
        text = text[:start] + text[start + len(m.group(0)):]
        text = section_add(text, NATIVE, line)
    elif dep not in text:
        sys.exit(f"{dep} not in {path}")
    if shim:
        pkg = "horizon-web-" + shim
        rel = os.path.relpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "shims", shim), crate)
        text = section_add(text, WEB, f'{pkg} = {{ path = "{rel}" }}')
    open(path, "w").write(text)
    if shim:
        alias = f"extern crate horizon_web_{shim.replace('-', '_')} as {dep.replace('-', '_')};"
        for root in roots(crate):
            src = open(root).read()
            if alias in src:
                continue
            lines = src.split("\n")
            i = 0
            while i < len(lines) and (lines[i].startswith("//!") or lines[i].startswith("#![") or lines[i] == ""):
                i += 1
            lines.insert(i, f"// The browser build's stand-in (web/shims).\n#[cfg(target_arch = \"wasm32\")]\n{alias}\n")
            open(root, "w").write("\n".join(lines))
            print("aliased in", root)


def roots(crate):
    text = open(os.path.join(crate, "Cargo.toml")).read()
    lib = re.search(r"^\[lib\][^\[]*?^path\s*=\s*\"([^\"]+)\"", text, re.M | re.S)
    if lib:
        return [os.path.normpath(os.path.join(crate, lib.group(1)))]
    p = os.path.join(crate, "src", "lib.rs")
    return [p] if os.path.exists(p) else [os.path.join(crate, "src", "main.rs")]


main()
