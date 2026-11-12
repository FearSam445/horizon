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
  // The game calls this when it stops (logging::show_on_page): the page stays.
  window.horizonFailed = function (message) {
    status.style.display = '';
    bar.parentNode.style.display = 'none';
    say(/^You quit/.test(message) ? message : 'Horizon stopped:\n' + message);
  };
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

  // The zip, and the files the game makes (worlds, options), are kept in
  // IndexedDB when the page may use it (a runner's sandbox often may not):
  // every access can throw, and then nothing is kept.
  var DB = 'horizon', STORE = 'files';
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
  function get(key) {
    return db().then(function (d) {
      return new Promise(function (ok) {
        try {
          var request = d.transaction(STORE).objectStore(STORE).get(key);
          request.onsuccess = function () { ok(request.result || null); };
          request.onerror = function () { ok(null); };
        } catch (e) { ok(null); }
      });
    }).catch(function () { return null; });
  }
  function put(key, value) {
    return db().then(function (d) {
      try { d.transaction(STORE, 'readwrite').objectStore(STORE).put(value, key); } catch (e) {}
    }).catch(function () {});
  }
  function forget(key) {
    return db().then(function (d) {
      try { d.transaction(STORE, 'readwrite').objectStore(STORE).delete(key); } catch (e) {}
    }).catch(function () {});
  }
  var remembered = function () { return get('game-files'); };
  var remember = function (file) { return put('game-files', file); };

  // The game's own files: back from the last visit before it starts, then kept
  // again whenever they changed: every 15 s, and as the page is hidden or left.
  async function restoreKept() {
    var block = await get('kept');
    if (!block) return;
    try {
      wasm_bindgen.restore_kept_files(new Uint8Array(block));
    } catch (e) {
      console.warn('Horizon: the kept worlds could not be restored', e);
    }
  }
  function keepFiles() {
    var keptAt = wasm_bindgen.kept_changes();
    function keep() {
      try {
        var now = wasm_bindgen.kept_changes();
        if (now === keptAt) return;
        keptAt = now;
        put('kept', wasm_bindgen.kept_files());
      } catch (e) {}
    }
    setInterval(keep, 15000);
    document.addEventListener('visibilitychange', function () {
      if (document.visibilityState === 'hidden') keep();
    });
    window.addEventListener('pagehide', keep);
  }

  // Asks for the zip; resolves to a File, or null to start without one.
  function choose(reason) {
    return new Promise(function (ok) {
      var box = document.createElement('div');
      box.id = 'horizon-files';
      box.innerHTML =
        '<p class="title">Your game files</p>' +
        '<p>' + (reason || 'Horizon needs the game files prepared from your own copy of Minecraft: ' +
        'zip the <b>26.3</b> folder the Horizon launcher made (game-files/horizon-client/26.3) and choose it here. ' +
        'Only the files the game uses are loaded, so your device keeps as much memory free as it can.') + '</p>' +
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

  // Hands the game the files of `file` it uses, one at a time, as they are
  // packed (page/zip_reader.js): nothing it doesn't use is copied.
  async function handOver(file) {
    say('Loading your game files…', 0.5);
    var handed = await HorizonZip.each(file, function (path, deflated, length, data) {
      wasm_bindgen.game_file(path, deflated, length, data);
    }, function (fraction) {
      say('Loading your game files…', 0.5 + 0.4 * fraction);
    });
    console.log('Horizon: ' + handed.files + ' game files (' +
      Math.round(handed.bytes / 1048576) + ' MiB packed), ' + handed.skipped + ' left out');
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
    await restoreKept();

    var file = await remembered(), reason = null;
    for (;;) {
      if (!file) {
        status.style.display = 'none';
        file = await choose(reason);
        status.style.display = '';
        if (file) remember(file);
      }
      if (file) {
        try {
          await handOver(file);
        } catch (error) {
          // A zip the game can't use: forget it and ask again.
          reason = String(error.message || error);
          forget('game-files');
          file = null;
          continue;
        }
      }
      break;
    }
    say('Starting…', 0.95);
    try {
      wasm_bindgen.boot();
    } catch (error) {
      if (!(error && String(error.message || error).indexOf('Using exceptions for control flow') >= 0)) throw error;
    }
    status.style.display = 'none';
    keepFiles();
    if (window.horizonFocus) window.horizonFocus();
  }
  start().catch(fail);
})();
