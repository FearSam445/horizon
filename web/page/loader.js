// Unpacks the game (gzip + base64 in #horizon-wasm) and starts it. The packed
// text is dropped as soon as it is decoded, so the page holds the game once.
(function () {
  var status = document.getElementById('horizon-status');
  function say(text, progress) {
    if (!status) return;
    status.firstChild.nodeValue = text;
    var bar = status.querySelector('.bar > i');
    if (bar && progress != null) bar.style.width = Math.round(progress * 100) + '%';
  }
  function fail(error) {
    console.error(error);
    say('Horizon could not start:\n' + (error && error.message ? error.message : error));
  }
  function fromBase64(text) {
    if (Uint8Array.fromBase64) return Uint8Array.fromBase64(text);
    var binary = atob(text), bytes = new Uint8Array(binary.length);
    for (var i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
    return bytes;
  }
  async function start() {
    var holder = document.getElementById('horizon-wasm');
    var packed = fromBase64(holder.textContent.trim());
    holder.remove();
    holder = null;
    say('Unpacking…', 0.3);
    var stream = new Blob([packed]).stream().pipeThrough(new DecompressionStream('gzip'));
    packed = null;
    var bytes = await new Response(stream).arrayBuffer();
    say('Compiling…', 0.6);
    var module = await WebAssembly.compile(bytes);
    bytes = null;
    say('Starting…', 0.9);
    window.horizonStatus = say;
    await wasm_bindgen({ module_or_path: module });
  }
  start().then(function () {
    if (status) status.style.display = 'none';
  }, function (error) {
    // winit's event loop leaves start() through an exception on purpose.
    if (error && String(error.message || error).indexOf('Using exceptions for control flow') >= 0) {
      if (status) status.style.display = 'none';
      return;
    }
    fail(error);
  });
})();
