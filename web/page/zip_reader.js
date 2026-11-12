// Reads a zip's table of contents and hands over the files the game uses,
// one at a time, as they are packed: nothing it doesn't use is copied.
var HorizonZip = (function () {
  function u16(v, at) { return v.getUint16(at, true); }
  function u32(v, at) { return v.getUint32(at, true); }
  async function bytes(blob, from, to) {
    return new Uint8Array(await blob.slice(from, to).arrayBuffer());
  }
  var utf8 = new TextDecoder('utf-8');
  var cp437 = (function () {
    try { return new TextDecoder('ibm866'); } catch (e) { return utf8; }
  })();

  // The entries of `blob`'s central directory: name, method, sizes, header offset.
  async function index(blob) {
    var tailLength = Math.min(blob.size, 65557);
    var tail = await bytes(blob, blob.size - tailLength, blob.size);
    var view = new DataView(tail.buffer);
    var end = -1;
    for (var i = tail.length - 22; i >= 0; i--) {
      if (u32(view, i) === 0x06054b50) { end = i; break; }
    }
    if (end < 0) throw new Error('this file is not a zip');
    var count = u16(view, end + 10);
    var size = u32(view, end + 12);
    var offset = u32(view, end + 16);
    if (count === 0xffff || size === 0xffffffff || offset === 0xffffffff) {
      throw new Error('this zip is too big (ZIP64): zip the 26.3 folder without horizon-data.bin');
    }
    var dir = await bytes(blob, offset, offset + size);
    var d = new DataView(dir.buffer);
    var entries = [];
    for (var at = 0, n = 0; n < count; n++) {
      if (u32(d, at) !== 0x02014b50) throw new Error('the zip\'s table of contents is damaged');
      var flags = u16(d, at + 8), method = u16(d, at + 10);
      var packed = u32(d, at + 20), length = u32(d, at + 24);
      var nameLength = u16(d, at + 28), extraLength = u16(d, at + 30), commentLength = u16(d, at + 32);
      var header = u32(d, at + 42);
      var raw = dir.subarray(at + 46, at + 46 + nameLength);
      var name = (flags & 0x800 ? utf8 : cp437).decode(raw).replace(/\\/g, '/');
      entries.push({ name: name, method: method, packed: packed, length: length, header: header, encrypted: (flags & 1) !== 0 });
      at += 46 + nameLength + extraLength + commentLength;
    }
    return entries;
  }

  // What the page leaves out: folders, macOS's extra files, the prepared data
  // file (never read in a page), and sounds while the page has no sound.
  function skipped(path) {
    var base = path.slice(path.lastIndexOf('/') + 1);
    return path.endsWith('/') || path.startsWith('__MACOSX/') || base.startsWith('._') ||
      base === '.DS_Store' || path === 'horizon-data.bin' ||
      path.startsWith('assets/minecraft/sounds/');
  }

  // The folder in the zip holding horizon-resources.json ('' for the zip's root).
  function rootOf(entries) {
    var best = null;
    entries.forEach(function (e) {
      if (e.name.startsWith('__MACOSX/')) return;
      var m = /^(.*\/)?horizon-resources\.json$/.exec(e.name);
      if (m && (best === null || (m[1] || '').length < best.length)) best = m[1] || '';
    });
    return best;
  }

  // Calls `give(path, deflated, length, packedBytes)` for each file the game uses
  // (path from the folder holding horizon-resources.json), reading the zip in
  // windows of `window` bytes; `progress(fraction)` as it goes.
  async function each(blob, give, progress, window) {
    window = window || (8 << 20);
    var entries = await index(blob);
    var root = rootOf(entries);
    if (root === null) {
      throw new Error('the zip holds no horizon-resources.json: zip the prepared 26.3 folder itself');
    }
    var wanted = entries.filter(function (e) {
      if (!e.name.startsWith(root)) return false;
      e.path = e.name.slice(root.length);
      return e.path !== '' && !skipped(e.path);
    }).sort(function (a, b) { return a.header - b.header; });
    var bad = wanted.find(function (e) { return e.encrypted || (e.method !== 0 && e.method !== 8); });
    if (bad) throw new Error(bad.name + ' is ' + (bad.encrypted ? 'encrypted' : 'packed in a way the page can\'t read') + ': zip the folder again without a password or options');
    var start = 0, chunk = new Uint8Array(0), view = new DataView(chunk.buffer), given = 0, total = 0;
    for (var k = 0; k < wanted.length; k++) {
      var e = wanted[k];
      // The local header's own name and extra lengths say where the data starts.
      if (e.header < start || e.header + 30 > start + chunk.length) {
        start = e.header;
        chunk = await bytes(blob, start, Math.min(blob.size, start + Math.max(window, 30 + 0x1ffff + e.packed)));
        view = new DataView(chunk.buffer);
      }
      var at = e.header - start;
      if (u32(view, at) !== 0x04034b50) throw new Error('the zip is damaged at ' + e.name);
      var dataAt = at + 30 + u16(view, at + 26) + u16(view, at + 28);
      if (dataAt + e.packed > chunk.length) {
        start = e.header;
        chunk = await bytes(blob, start, Math.min(blob.size, start + (dataAt - at) + e.packed));
        view = new DataView(chunk.buffer);
        at = 0;
        dataAt = 30 + u16(view, 26) + u16(view, 28);
      }
      give(e.path, e.method === 8, e.length, chunk.subarray(dataAt, dataAt + e.packed));
      given++;
      total += e.packed;
      if (progress && (k & 255) === 0) progress(k / wanted.length);
    }
    return { files: given, bytes: total, skipped: entries.length - given };
  }

  return { index: index, each: each, skipped: skipped, rootOf: rootOf };
})();
if (typeof module !== 'undefined') module.exports = HorizonZip;
