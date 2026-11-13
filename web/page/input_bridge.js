// Eaglercraft runners (and their controller scripts) send synthetic events:
// keydown/keyup on window and mouse events on the game canvas. winit listens for
// keyboard events on the canvas and for pointer events, not mouse events, so the
// synthetic ones are passed on in that form. Real key presses that miss the
// canvas (it lost focus) are passed on too, and a click gives it focus back:
// on an iPad with a keyboard, keys only reach the focused element.
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
      // winit reads a move's motion from getCoalescedEvents(), which is empty for an
      // event made by a script (Chrome, current Safari): the look would never turn.
      try { Object.defineProperty(ev, 'getCoalescedEvents', { value: function () { return [ev]; } }); } catch (_) {}
      target.dispatchEvent(ev);
    }, true);
  });
  // Focus follows the game: at start (horizonFocus) and on every press.
  window.horizonFocus = function () {
    var target = canvas();
    if (target && document.activeElement !== target) {
      try { target.focus({ preventScroll: true }); } catch (_) { target.focus(); }
    }
  };
  document.addEventListener('pointerdown', function (e) {
    if (e.target === canvas()) window.horizonFocus();
  }, true);
  ['keydown', 'keyup'].forEach(function (type) {
    window.addEventListener(type, function (e) {
      if (e[MARK] || pauseKey(e) || realEscape(e)) return;
      var target = canvas();
      if (!target || e.target === target) return;
      // A key typed into the page's own controls (the file picker, the touch
      // controls' typing field) stays there.
      if (e.target && e.target.closest && e.target.closest('#horizon-files, #horizon-touch')) return;
      if (e.isTrusted) e.preventDefault();
      var ev = new KeyboardEvent(type, {
        key: e.key, code: e.code, location: e.location, repeat: e.repeat,
        ctrlKey: e.ctrlKey, shiftKey: e.shiftKey, altKey: e.altKey, metaKey: e.metaKey,
        bubbles: true, cancelable: true, composed: true
      });
      ev[MARK] = true;
      target.dispatchEvent(ev);
    }, true);
  });
  // The pause key is ~ (the ` key, with or without Shift): it works as Escape
  // (pause, close a menu), as in Eaglercraft, and many iPad keyboards have no
  // Escape key. Matched by the key's place and by what it types, as keyboards
  // of other layouts name it differently.
  function pauseKey(e) {
    return e.code === 'Backquote' || e.key === '`' || e.key === '~' ||
      (e.key === 'Dead' && e.code === 'Backquote');
  }
  // The keyboard's own Escape doesn't reach the game (the owner's choice: ~
  // pauses); a runner's controller still pauses through its synthetic Escape.
  function realEscape(e) {
    return e.isTrusted && (e.key === 'Escape' || e.code === 'Escape');
  }
  ['keydown', 'keyup'].forEach(function (type) {
    document.addEventListener(type, function (e) {
      if (e[MARK]) return;
      var target = canvas();
      if (!target) return;
      if (realEscape(e)) {
        e.preventDefault();
        e.stopImmediatePropagation();
        return;
      }
      if (!pauseKey(e)) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      var ev = new KeyboardEvent(type, {
        key: 'Escape', code: 'Escape', repeat: e.repeat, bubbles: true, cancelable: true, composed: true
      });
      ev[MARK] = true;
      target.dispatchEvent(ev);
    }, true);
  });
})();
