#!/usr/bin/env python3
"""Lists WGSL uniforms whose size is not a multiple of 16 bytes, which WebGL2
can't bind (wgpu: BUFFER_BINDINGS_NOT_16_BYTE_ALIGNED is missing there).

    python3 web/tools/uniforms.py [ROOT]
"""
import glob
import re
import sys

SCALAR = {"f32": (4, 4), "u32": (4, 4), "i32": (4, 4), "f16": (2, 2), "bool": (4, 4)}


def size_align(t, structs):
    t = t.strip()
    if t in SCALAR:
        return SCALAR[t]
    m = re.fullmatch(r"vec([234])(?:<(\w+)>|([fiuh]))", t)
    if m:
        n = int(m.group(1))
        s = SCALAR[m.group(2) or {"f": "f32", "i": "i32", "u": "u32", "h": "f16"}[m.group(3)]][0]
        return s * n, {2: 2 * s, 3: 4 * s, 4: 4 * s}[n]
    m = re.fullmatch(r"mat([234])x([234])(?:<\w+>|f)", t)
    if m:
        cols, rows = int(m.group(1)), int(m.group(2))
        align = {2: 8, 3: 16, 4: 16}[rows]
        return cols * align, align
    m = re.fullmatch(r"array<(.+),\s*(\d+)u?>", t)
    if m:
        s, a = size_align(m.group(1), structs)
        a = max(a, 16)  # uniform arrays: 16-byte element stride
        stride = -(-s // a) * a
        return stride * int(m.group(2)), a
    if t in structs:
        return structs[t]
    raise KeyError(t)


def check(path):
    src = re.sub(r"//[^\n]*", "", open(path).read())
    for name, value in re.findall(r"const\s+(\w+)\s*(?::\s*\w+)?\s*=\s*(\d+)u?\s*;", src):
        src = re.sub(rf"\b{name}\b", value, src)
    structs, out = {}, []
    for name, body in re.findall(r"struct\s+(\w+)\s*\{([^}]*)\}", src):
        try:
            offset, align = 0, 1
            fields, depth, cur = [], 0, ""
            for ch in body:
                depth += {"<": 1, ">": -1}.get(ch, 0)
                if ch == "," and depth == 0:
                    fields.append(cur)
                    cur = ""
                else:
                    cur += ch
            fields.append(cur)
            for field in [f for f in fields if ":" in f]:
                _, ftype = field.rsplit(":", 1) if field.count(":") == 1 else field.split(":", 1)
                ftype = re.sub(r"@\w+\([^)]*\)", "", ftype).strip()
                s, a = size_align(ftype, structs)
                offset = -(-offset // a) * a + s
                align = max(align, a)
            structs[name] = (-(-offset // align) * align, align)
        except KeyError:
            pass
    for var, ty in re.findall(r"var<uniform>\s+(\w+)\s*:\s*([^;]+);", src):
        try:
            size = size_align(ty, structs)[0]
        except KeyError:
            out.append(f"{path}: {var}: {ty.strip()}: size unknown")
            continue
        if size % 16:
            out.append(f"{path}: {var}: {ty.strip()} is {size} bytes")
    return out


root = sys.argv[1] if len(sys.argv) > 1 else "."
problems = [p for f in sorted(glob.glob(f"{root}/crates/*/src/**/*.wgsl", recursive=True)) for p in check(f)]
print("\n".join(problems) or "every uniform is a multiple of 16 bytes")
