// Unpacks the game (gzip + base64 in #horizon-wasm), asks for the player's
// prepared game files (a zip of the 26.3 folder) when none are remembered, and
// starts it. The packed text and the zip's JavaScript copies are dropped as soon
// as the game holds them, so the page keeps each once.
(function () {
  var CHUNK = 8 << 20;
  var status = document.getElementById('horizon-status');
  var text = status.firstChild;
  var bar = status.querySelector('.bar > i');
  function say(message, progress) {
    text.nodeValue = message;
    if (progress != null) bar.style.width = Math.round(progress * 100) + '%';
  }
  function fail(error) {
    console.error(error);
    status.style.display = '';
    say('Horizon could not start:\n' + (error && error.message ? error.message : error));
  }
  function fromBase64(b64) {
    if (Uint8Array.fromBase64) return Uint8Array.fromBase64(b64);
    var binary = atob(b64), bytes = new Uint8Array(binary.length);
    for (var i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
    return bytes;
  }

  // The zip is remembered in IndexedDB when the page may use it (a runner's
  // sandbox often may not): every access can throw, and then nothing is kept.
  var DB = 'horizon', STORE = 'files', KEY = 'game-files';
  function db() {
    return new Promise(function (ok, no) {
      try {
        var open = indexedDB.open(DB, 1);
        open.onupgradeneeded = function () { open.result.createObjectStore(STORE); };
        open.onsuccess = function () { ok(open.result); };
        open.onerror = function () { no(open.error); };
      } catch (e) { no(e); }
    });
  }
  function remembered() {
    return db().then(function (d) {
      return new Promise(function (ok) {
        try {
          var get = d.transaction(STORE).objectStore(STORE).get(KEY);
          get.onsuccess = function () { ok(get.result || null); };
          get.onerror = function () { ok(null); };
        } catch (e) { ok(null); }
      });
    }).catch(function () { return null; });
  }
  function remember(file) {
    return db().then(function (d) {
      try { d.transaction(STORE, 'readwrite').objectStore(STORE).put(file, KEY); } catch (e) {}
    }).catch(function () {});
  }
  function forget() {
    return db().then(function (d) {
      try { d.transaction(STORE, 'readwrite').objectStore(STORE).delete(KEY); } catch (e) {}
    }).catch(function () {});
  }

  // Asks for the zip; resolves to a File, or null to start without one.
  function choose(reason) {
    return new Promise(function (ok) {
      var box = document.createElement('div');
      box.id = 'horizon-files';
      box.innerHTML =
        '<p class="title">Your game files</p>' +
        '<p>' + (reason || 'Horizon needs the game files prepared from your own copy of Minecraft: ' +
        'zip the <b>26.3</b> folder the Horizon launcher made (game-files/horizon-client/26.3) and choose it here.') + '</p>' +
        '<label class="button">Choose zip…<input type="file" accept=".zip,application/zip" hidden></label>' +
        '<button class="link" type="button">Start without them</button>';
      document.body.appendChild(box);
      box.querySelector('input').addEventListener('change', function (e) {
        var file = e.target.files[0];
        if (!file) return;
        box.remove();
        ok(file);
      });
      box.querySelector('button').addEventListener('click', function () {
        box.remove();
        ok(null);
      });
    });
  }

  // Copies `file` into the game a piece at a time: one copy of the zip, in the game.
  async function handOver(file) {
    wasm_bindgen.game_files_begin(file.size);
    for (var at = 0; at < file.size; at += CHUNK) {
      say('Loading your game files…', 0.5 + 0.4 * at / file.size);
      var piece = new Uint8Array(await file.slice(at, at + CHUNK).arrayBuffer());
      wasm_bindgen.game_files_chunk(piece);
    }
  }

  async function start() {
    var holder = document.getElementById('horizon-wasm');
    var packed = fromBase64(holder.textContent.trim());
    holder.remove();
    holder = null;
    say('Unpacking…', 0.1);
    var stream = new Blob([packed]).stream().pipeThrough(new DecompressionStream('gzip'));
    packed = null;
    var bytes = await new Response(stream).arrayBuffer();
    say('Compiling…', 0.3);
    var module = await WebAssembly.compile(bytes);
    bytes = null;
    await wasm_bindgen({ module_or_path: module });
    module = null;

    var file = await remembered(), reason = null;
    for (;;) {
      if (!file) {
        status.style.display = 'none';
        file = await choose(reason);
        status.style.display = '';
        if (file) remember(file);
      }
      if (file) await handOver(file);
      say('Starting…', 0.95);
      try {
        wasm_bindgen.boot();
        break;
      } catch (error) {
        if (error && String(error.message || error).indexOf('Using exceptions for control flow') >= 0) break;
        if (!file) throw error;
        // A zip the game can't use: forget it and ask again.
        reason = String(error.message || error);
        forget();
        file = null;
      }
    }
    status.style.display = 'none';
  }
  start().catch(fail);
})();
