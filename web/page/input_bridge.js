// Eaglercraft runners (and their controller scripts) send synthetic events:
// keydown/keyup on window and mouse events on the game canvas. winit listens for
// keyboard events on the canvas and for pointer events, not mouse events, so the
// synthetic ones are passed on in that form. Real input is left alone.
(function () {
  var MARK = '__horizonBridged';
  function canvas() {
    return document.querySelector('canvas._eaglercraftX_canvas_element');
  }
  function copyMouse(e, extra) {
    var init = {
      bubbles: true, cancelable: true, composed: true, view: window,
      clientX: e.clientX, clientY: e.clientY, screenX: e.screenX, screenY: e.screenY,
      button: e.button, buttons: e.buttons,
      movementX: e.movementX || 0, movementY: e.movementY || 0,
      ctrlKey: e.ctrlKey, shiftKey: e.shiftKey, altKey: e.altKey, metaKey: e.metaKey,
      pointerId: 1, pointerType: 'mouse', isPrimary: true
    };
    for (var k in extra) init[k] = extra[k];
    return init;
  }
  var POINTER = { mousedown: 'pointerdown', mouseup: 'pointerup', mousemove: 'pointermove' };
  Object.keys(POINTER).forEach(function (type) {
    document.addEventListener(type, function (e) {
      if (e.isTrusted || e[MARK]) return;
      var target = canvas();
      if (!target || e.target !== target) return;
      var ev = new PointerEvent(POINTER[type], copyMouse(e, {}));
      ev[MARK] = true;
      // movementX/Y from defineProperty'd runner events are copied too.
      try { Object.defineProperty(ev, 'movementX', { value: e.movementX || 0 }); } catch (_) {}
      try { Object.defineProperty(ev, 'movementY', { value: e.movementY || 0 }); } catch (_) {}
      target.dispatchEvent(ev);
    }, true);
  });
  ['keydown', 'keyup'].forEach(function (type) {
    window.addEventListener(type, function (e) {
      if (e.isTrusted || e[MARK]) return;
      var target = canvas();
      if (!target || e.target === target) return;
      var ev = new KeyboardEvent(type, {
        key: e.key, code: e.code, location: e.location, repeat: e.repeat,
        ctrlKey: e.ctrlKey, shiftKey: e.shiftKey, altKey: e.altKey, metaKey: e.metaKey,
        bubbles: true, cancelable: true, composed: true
      });
      ev[MARK] = true;
      target.dispatchEvent(ev);
    }, true);
  });
})();
