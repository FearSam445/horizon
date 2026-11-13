// Touch controls for tablets and phones, drawn over the game and turned into the
// keyboard and mouse events the game already understands (sent to the canvas,
// as the input bridge sends a runner's).
//
// In a world (the pointer is locked): a joystick on the left half moves (pushed
// to its edge it sprints), dragging on the right half looks around, a tap there
// uses or places, and holding still there breaks or attacks. Buttons: jump,
// sneak (a toggle), attack, inventory, drop, chat, camera and pause; tapping the
// hotbar picks a slot. In menus a tap clicks, a drag drags (sliders, items), a
// long press right-clicks, two fingers scroll, and a keyboard button types.
//
// It shows itself on the first touch and hides when a real mouse moves, so a
// keyboard and mouse play as before.
(function () {
  var MARK = '__horizonBridged';
  var canvas = function () { return document.querySelector('canvas._eaglercraftX_canvas_element'); };

  // ---- Pointer lock: a touch has none, so the game is told it has it. ----
  var fakeLocked = null;
  var touchMode = false;
  (function () {
    var nativeRequest = Element.prototype.requestPointerLock;
    var nativeExit = Document.prototype.exitPointerLock;
    var desc = Object.getOwnPropertyDescriptor(Document.prototype, 'pointerLockElement');
    function changed() {
      setTimeout(function () { document.dispatchEvent(new Event('pointerlockchange')); }, 0);
    }
    Element.prototype.requestPointerLock = function () {
      if (touchMode || !nativeRequest) {
        fakeLocked = this;
        changed();
        return Promise.resolve();
      }
      return nativeRequest.apply(this, arguments);
    };
    Document.prototype.exitPointerLock = function () {
      if (fakeLocked) {
        fakeLocked = null;
        changed();
        return;
      }
      if (nativeExit) return nativeExit.apply(this, arguments);
    };
    try {
      Object.defineProperty(Document.prototype, 'pointerLockElement', {
        configurable: true,
        get: function () { return fakeLocked || (desc && desc.get ? desc.get.call(this) : null); }
      });
    } catch (e) {}
  })();
  function inWorld() {
    var c = canvas();
    return !!c && document.pointerLockElement === c;
  }

  // ---- Events to the game ----
  var CODE_KEY = {
    KeyW: 'w', KeyA: 'a', KeyS: 's', KeyD: 'd', KeyE: 'e', KeyQ: 'q', KeyT: 't',
    Space: ' ', ShiftLeft: 'Shift', ControlLeft: 'Control', Escape: 'Escape', F5: 'F5',
    Enter: 'Enter', Backspace: 'Backspace', Tab: 'Tab'
  };
  function key(code, down, keyName) {
    var c = canvas();
    if (!c) return;
    var name = keyName || CODE_KEY[code] || (code.indexOf('Digit') === 0 ? code.slice(5) : code);
    var ev = new KeyboardEvent(down ? 'keydown' : 'keyup', {
      key: name, code: code, bubbles: true, cancelable: true, composed: true
    });
    ev[MARK] = true;
    c.dispatchEvent(ev);
  }
  var held = {};
  function hold(code, down) {
    if (!!held[code] === down) return;
    held[code] = down;
    key(code, down);
  }
  function tapKey(code) {
    key(code, true);
    setTimeout(function () { key(code, false); }, 60);
  }
  var pointer = { x: 0, y: 0, buttons: 0 };
  function mouse(type, x, y, button, dx, dy) {
    var c = canvas();
    if (!c) return;
    if (x != null) { pointer.x = x; pointer.y = y; }
    if (type === 'mousedown') pointer.buttons |= (button === 2 ? 2 : 1);
    if (type === 'mouseup') pointer.buttons &= ~(button === 2 ? 2 : 1);
    var ev = new MouseEvent(type, {
      bubbles: true, cancelable: true, composed: true, view: window,
      clientX: pointer.x, clientY: pointer.y, screenX: pointer.x, screenY: pointer.y,
      button: button || 0, buttons: pointer.buttons
    });
    try { Object.defineProperty(ev, 'movementX', { value: dx || 0 }); } catch (e) {}
    try { Object.defineProperty(ev, 'movementY', { value: dy || 0 }); } catch (e) {}
    c.dispatchEvent(ev);
  }
  function click(button, x, y) {
    mouse('mousemove', x, y);
    mouse('mousedown', x, y, button);
    setTimeout(function () { mouse('mouseup', x, y, button); }, 50);
  }
  function wheel(x, y, dy) {
    var c = canvas();
    if (!c) return;
    c.dispatchEvent(new WheelEvent('wheel', {
      bubbles: true, cancelable: true, clientX: x, clientY: y, deltaY: dy, deltaMode: 0
    }));
  }
  function focusGame() {
    if (typing) return;
    if (window.horizonFocus) window.horizonFocus();
  }

  // ---- The overlay ----
  var css = '\
#horizon-touch{position:fixed;inset:0;z-index:5;touch-action:none;-webkit-user-select:none;user-select:none;\
 -webkit-touch-callout:none;display:none;font-family:system-ui,-apple-system,sans-serif}\
#horizon-touch.on{display:block}\
#horizon-touch .b{position:absolute;display:flex;align-items:center;justify-content:center;box-sizing:border-box;\
 border-radius:50%;background:rgba(18,20,26,.38);border:1.5px solid rgba(255,255,255,.32);color:#fff;\
 -webkit-backdrop-filter:blur(6px);backdrop-filter:blur(6px);transition:transform .08s,background .08s;pointer-events:auto}\
#horizon-touch .b svg{width:46%;height:46%;fill:none;stroke:#fff;stroke-width:2.2;stroke-linecap:round;stroke-linejoin:round;opacity:.92}\
#horizon-touch .b.down{background:rgba(255,255,255,.3);transform:scale(.92)}\
#horizon-touch .b.on{background:rgba(240,121,90,.55);border-color:rgba(255,200,180,.7)}\
#horizon-touch .sq{border-radius:14px}\
#horizon-touch .play,#horizon-touch .menu{display:none}\
#horizon-touch.world .play{display:flex}\
#horizon-touch.menus .menu{display:flex}\
#horizon-touch .stick{position:absolute;width:var(--stick);height:var(--stick);margin:calc(var(--stick)/-2) 0 0 calc(var(--stick)/-2);\
 border-radius:50%;background:rgba(18,20,26,.28);border:1.5px solid rgba(255,255,255,.28);display:none;pointer-events:none}\
#horizon-touch .knob{position:absolute;left:50%;top:50%;width:44%;height:44%;margin:-22% 0 0 -22%;border-radius:50%;\
 background:rgba(255,255,255,.55);box-shadow:0 2px 10px rgba(0,0,0,.35)}\
#horizon-touch .stick.sprint .knob{background:rgba(240,121,90,.85)}\
#horizon-touch .hint{position:absolute;color:rgba(255,255,255,.55);font-size:12px;letter-spacing:.02em;pointer-events:none;\
 text-shadow:0 1px 3px rgba(0,0,0,.6)}\
#horizon-touch input{position:absolute;left:-9999px;top:0;width:1px;height:1px;opacity:0;font-size:16px}';
  var ICONS = {
    jump: '<svg viewBox="0 0 24 24"><path d="M12 19V5M5 12l7-7 7 7"/></svg>',
    sneak: '<svg viewBox="0 0 24 24"><path d="M12 5v14M5 12l7 7 7-7"/></svg>',
    attack: '<svg viewBox="0 0 24 24"><path d="M14.5 17.5 3 6V3h3l11.5 11.5M13 19l6-6M16 16l4 4M19 21l2-2"/></svg>',
    use: '<svg viewBox="0 0 24 24"><rect x="4" y="4" width="16" height="16" rx="2"/><path d="M12 8v8M8 12h8"/></svg>',
    inv: '<svg viewBox="0 0 24 24"><circle cx="6" cy="12" r="1.2"/><circle cx="12" cy="12" r="1.2"/><circle cx="18" cy="12" r="1.2"/></svg>',
    drop: '<svg viewBox="0 0 24 24"><path d="M12 3v12M7 10l5 5 5-5M5 21h14"/></svg>',
    chat: '<svg viewBox="0 0 24 24"><path d="M21 12a8 8 0 0 1-11.6 7.1L3 21l1.9-6.4A8 8 0 1 1 21 12z"/></svg>',
    camera: '<svg viewBox="0 0 24 24"><path d="M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12z"/><circle cx="12" cy="12" r="3"/></svg>',
    pause: '<svg viewBox="0 0 24 24"><path d="M9 5v14M15 5v14"/></svg>',
    back: '<svg viewBox="0 0 24 24"><path d="M18 6 6 18M6 6l12 12"/></svg>',
    keyboard: '<svg viewBox="0 0 24 24"><rect x="2" y="6" width="20" height="12" rx="2"/><path d="M6 10h.01M10 10h.01M14 10h.01M18 10h.01M7 14h10"/></svg>'
  };
  var root, stickEl, knobEl, input, typing = false;
  var buttons = [];
  function button(name, cls, label) {
    var el = document.createElement('div');
    el.className = 'b ' + cls;
    el.innerHTML = ICONS[name];
    el.setAttribute('aria-label', label);
    el.dataset.name = name;
    root.appendChild(el);
    buttons.push(el);
    return el;
  }
  function build() {
    var style = document.createElement('style');
    style.textContent = css;
    document.head.appendChild(style);
    root = document.createElement('div');
    root.id = 'horizon-touch';
    document.body.appendChild(root);
    stickEl = document.createElement('div');
    stickEl.className = 'stick';
    knobEl = document.createElement('div');
    knobEl.className = 'knob';
    stickEl.appendChild(knobEl);
    root.appendChild(stickEl);
    button('jump', 'play', 'Jump');
    button('sneak', 'play', 'Sneak');
    button('attack', 'play', 'Attack');
    button('use', 'play', 'Use');
    button('inv', 'play sq', 'Inventory');
    button('drop', 'play sq', 'Drop item');
    button('chat', 'play sq', 'Chat');
    button('camera', 'play sq', 'Camera');
    button('pause', 'play sq', 'Pause');
    button('back', 'menu sq', 'Back');
    button('keyboard', 'menu sq', 'Keyboard');
    var hint = document.createElement('div');
    hint.className = 'hint play';
    hint.id = 'horizon-touch-hint';
    hint.textContent = 'Drag to look · tap to use · hold to break';
    root.appendChild(hint);
    input = document.createElement('input');
    input.type = 'text';
    input.autocapitalize = 'off';
    input.setAttribute('autocorrect', 'off');
    input.setAttribute('autocomplete', 'off');
    input.spellcheck = false;
    root.appendChild(input);
    wireTyping();
    layout();
    window.addEventListener('resize', layout);
    root.addEventListener('touchstart', onStart, { passive: false });
    root.addEventListener('touchmove', onMove, { passive: false });
    root.addEventListener('touchend', onEnd, { passive: false });
    root.addEventListener('touchcancel', onEnd, { passive: false });
    root.addEventListener('contextmenu', function (e) { e.preventDefault(); });
  }

  // ---- Layout: sized to the screen, clear of the hotbar and the safe areas ----
  var geometry = {};
  function place(name, x, y, size) {
    var el = root.querySelector('.b[data-name="' + name + '"]');
    el.style.left = (x - size / 2) + 'px';
    el.style.top = (y - size / 2) + 'px';
    el.style.width = el.style.height = size + 'px';
  }
  function layout() {
    var w = window.innerWidth, h = window.innerHeight;
    var u = Math.max(44, Math.min(w, h) * 0.105);   // a button: ~86 px on an iPad
    var small = Math.round(u * 0.62);
    var pad = Math.max(14, u * 0.22);
    var hot = hotbar();
    var bottom = h - pad - u / 2;                    // the lowest row's centre
    var right = w - pad - u / 2;
    root.style.setProperty('--stick', Math.round(u * 1.7) + 'px');
    // The action cluster, bottom right, above the hotbar's height.
    place('jump', right, bottom - u * 0.1, u * 1.12);
    place('sneak', right - u * 1.3, bottom + u * 0.12, u * 0.9);
    place('attack', right - u * 0.35, bottom - u * 1.35, u * 0.92);
    place('use', right - u * 1.55, bottom - u * 1.05, u * 0.82);
    // Small squares: inventory right of the hotbar, drop left of it; pause, camera
    // and chat top left, clear of the game's toasts (top right); in menus, back
    // and the keyboard there too (the chat box spans the bottom).
    var hy = hot ? hot.top + hot.height / 2 : h - pad - small / 2;
    var gap = small * 0.62;
    place('inv', hot ? hot.right + gap : w / 2 + u * 3, hy, small);
    place('drop', hot ? hot.left - gap : w / 2 - u * 3, hy, small);
    var top = pad + small / 2;
    place('pause', pad + small / 2, top, small);
    place('camera', pad + small * 1.65, top, small);
    place('chat', pad + small * 2.8, top, small);
    place('back', pad + small / 2, top, small);
    place('keyboard', pad + small * 1.65, top, small);
    var hint = document.getElementById('horizon-touch-hint');
    hint.style.left = '0';
    hint.style.right = '0';
    hint.style.textAlign = 'center';
    hint.style.justifyContent = 'center';
    hint.style.top = (top - 7) + 'px';
    geometry = { w: w, h: h, u: u };
  }
  // The hotbar as the game draws it: 182 x 22 GUI pixels, centred at the bottom,
  // at Minecraft's GUI scale (the option, or the largest that leaves 320 x 240).
  function hotbar() {
    var c = canvas();
    if (!c || !c.width) return null;
    var fw = c.width, fh = c.height, max = 0;
    try { max = window.wasm_bindgen && wasm_bindgen.gui_scale_setting ? wasm_bindgen.gui_scale_setting() : 0; } catch (e) {}
    var s = 1;
    while (s !== max && s < fw && s < fh && Math.floor(fw / (s + 1)) >= 320 && Math.floor(fh / (s + 1)) >= 240) s++;
    var r = c.getBoundingClientRect();
    var css = s * r.width / fw;
    var width = 182 * css, height = 22 * css;
    var left = r.left + (r.width - width) / 2;
    return { left: left, right: left + width, top: r.bottom - height, height: height, slot: 20 * css };
  }
  function hotbarSlot(x, y) {
    var hot = hotbar();
    if (!hot || y < hot.top || x < hot.left || x > hot.right) return 0;
    return Math.min(9, Math.max(1, Math.floor((x - hot.left - hot.slot * 0.05) / hot.slot) + 1));
  }

  // ---- Touches ----
  var touches = {};       // id -> what it does
  var LOOK = 1.5;         // view movement per CSS pixel dragged
  var TAP_MS = 260, HOLD_MS = 300, LONG_MS = 480, SLOP = 10;
  var sneaking = false;
  function buttonAt(x, y) {
    for (var i = 0; i < buttons.length; i++) {
      var b = buttons[i];
      if (b.offsetParent === null) continue;
      var r = b.getBoundingClientRect();
      var cx = r.left + r.width / 2, cy = r.top + r.height / 2, rad = r.width / 2 + 6;
      if (b.classList.contains('sq') ? (x >= r.left - 6 && x <= r.right + 6 && y >= r.top - 6 && y <= r.bottom + 6)
        : ((x - cx) * (x - cx) + (y - cy) * (y - cy) <= rad * rad)) return b;
    }
    return null;
  }
  function onStart(e) {
    e.preventDefault();
    if (!touchMode) setTouchMode(true);
    var now = performance.now();
    for (var i = 0; i < e.changedTouches.length; i++) {
      var t = e.changedTouches[i], x = t.clientX, y = t.clientY;
      var b = buttonAt(x, y);
      if (b) { touches[t.identifier] = { kind: 'button', el: b }; press(b, true); continue; }
      if (!inWorld()) { menuStart(t, now); continue; }
      focusGame();
      var slot = hotbarSlot(x, y);
      if (slot) { tapKey('Digit' + slot); touches[t.identifier] = { kind: 'hotbar' }; continue; }
      if (x < geometry.w * 0.42 && !stickTouch()) {
        touches[t.identifier] = { kind: 'stick', x0: x, y0: y };
        stickEl.style.left = x + 'px';
        stickEl.style.top = y + 'px';
        stickEl.style.display = 'block';
        knobEl.style.transform = '';
        continue;
      }
      var look = { kind: 'look', x0: x, y0: y, x: x, y: y, t0: now, moved: false, breaking: false };
      look.timer = setTimeout(function (l) { return function () {
        if (!l.moved && touches[l.id] === l) { l.breaking = true; mouse('mousedown', null, null, 0); }
      }; }(look), HOLD_MS);
      look.id = t.identifier;
      touches[t.identifier] = look;
    }
  }
  function stickTouch() {
    for (var id in touches) if (touches[id].kind === 'stick') return touches[id];
    return null;
  }
  function onMove(e) {
    e.preventDefault();
    for (var i = 0; i < e.changedTouches.length; i++) {
      var t = e.changedTouches[i], s = touches[t.identifier];
      if (!s) continue;
      var x = t.clientX, y = t.clientY;
      if (s.kind === 'stick') stickMove(s, x, y);
      else if (s.kind === 'look') {
        var dx = x - s.x, dy = y - s.y;
        s.x = x; s.y = y;
        if (!s.moved && Math.abs(x - s.x0) + Math.abs(y - s.y0) > SLOP) s.moved = true;
        if (s.moved && (dx || dy)) {
          mouse('mousemove', null, null, 0, dx * LOOK, dy * LOOK);
          hideHint();
        }
      } else if (s.kind === 'menu') menuMove(s, t, x, y);
    }
  }
  function onEnd(e) {
    e.preventDefault();
    var now = performance.now();
    for (var i = 0; i < e.changedTouches.length; i++) {
      var t = e.changedTouches[i], s = touches[t.identifier];
      if (!s) continue;
      delete touches[t.identifier];
      if (s.kind === 'button') press(s.el, false);
      else if (s.kind === 'stick') stickEnd();
      else if (s.kind === 'look') {
        clearTimeout(s.timer);
        if (s.breaking) mouse('mouseup', null, null, 0);
        else if (!s.moved && now - s.t0 < TAP_MS + HOLD_MS) click(2);
      } else if (s.kind === 'menu') menuEnd(s, t, now);
    }
  }

  // The first-time hint goes once the player has looked around.
  function hideHint() {
    var hint = document.getElementById('horizon-touch-hint');
    if (hint && !hint.hidden) {
      hint.hidden = true;
      hint.style.display = 'none';
    }
  }

  // ---- The joystick ----
  function stickMove(s, x, y) {
    var r = parseFloat(getComputedStyle(root).getPropertyValue('--stick')) / 2 || 70;
    var dx = x - s.x0, dy = y - s.y0, d = Math.sqrt(dx * dx + dy * dy);
    var k = d > r ? r / d : 1;
    knobEl.style.transform = 'translate(' + dx * k + 'px,' + dy * k + 'px)';
    var nx = dx / r, ny = dy / r;
    var dead = 0.28;
    hold('KeyW', ny < -dead);
    hold('KeyS', ny > dead);
    hold('KeyA', nx < -dead);
    hold('KeyD', nx > dead);
    // Pushed to the edge, mostly forward: sprint (vanilla's Ctrl with W).
    var sprint = d > r * 0.92 && ny < -0.6;
    hold('ControlLeft', sprint);
    stickEl.classList.toggle('sprint', sprint);
  }
  function stickEnd() {
    ['KeyW', 'KeyA', 'KeyS', 'KeyD', 'ControlLeft'].forEach(function (c) { hold(c, false); });
    stickEl.style.display = 'none';
    stickEl.classList.remove('sprint');
  }

  // ---- Buttons ----
  function press(b, down) {
    var name = b.dataset.name;
    if (name !== 'sneak') b.classList.toggle('down', down);
    if (name === 'jump') { focusGame(); hold('Space', down); return; }
    if (name === 'attack') { focusGame(); if (down) mouse('mousedown', null, null, 0); else mouse('mouseup', null, null, 0); return; }
    if (name === 'use') { focusGame(); if (down) mouse('mousedown', null, null, 2); else mouse('mouseup', null, null, 2); return; }
    if (!down) return;
    if (name === 'sneak') {
      sneaking = !sneaking;
      b.classList.toggle('on', sneaking);
      hold('ShiftLeft', sneaking);
    } else if (name === 'inv') tapKey('KeyE');
    else if (name === 'drop') tapKey('KeyQ');
    else if (name === 'chat') {
      tapKey('KeyT');
      // The device's keyboard comes up with it (iOS shows it only for a focus
      // made during the touch itself).
      startTyping();
    }
    else if (name === 'camera') tapKey('F5');
    else if (name === 'pause' || name === 'back') { stopTyping(); tapKey('Escape'); }
    else if (name === 'keyboard') { if (typing) stopTyping(); else startTyping(); }
  }

  // ---- Menus: the touch as a mouse ----
  function menuStart(t, now) {
    var s = { kind: 'menu', x0: t.clientX, y0: t.clientY, t0: now, dragging: false, done: false, id: t.identifier };
    touches[t.identifier] = s;
    var fingers = Object.keys(touches).filter(function (id) { return touches[id].kind === 'menu'; });
    if (fingers.length === 2) {
      // Two fingers scroll: neither clicks.
      fingers.forEach(function (id) { touches[id].scroll = true; touches[id].done = true; clearTimeout(touches[id].timer); });
      s.lastY = t.clientY;
      return;
    }
    mouse('mousemove', t.clientX, t.clientY);
    s.timer = setTimeout(function () {
      if (touches[s.id] === s && !s.dragging && !s.done) { s.done = true; click(2, s.x0, s.y0); }
    }, LONG_MS);
  }
  function menuMove(s, t, x, y) {
    if (s.scroll) {
      var last = s.lastY == null ? y : s.lastY;
      s.lastY = y;
      if (Math.abs(y - last) > 0) wheel(x, y, (last - y) * 2.5);
      return;
    }
    if (s.done) return;
    if (!s.dragging && Math.abs(x - s.x0) + Math.abs(y - s.y0) > SLOP) {
      s.dragging = true;
      clearTimeout(s.timer);
      mouse('mousemove', s.x0, s.y0);
      mouse('mousedown', s.x0, s.y0, 0);
    }
    mouse('mousemove', x, y);
  }
  function menuEnd(s, t, now) {
    clearTimeout(s.timer);
    if (s.dragging) { mouse('mouseup', t.clientX, t.clientY, 0); return; }
    if (!s.done) click(0, s.x0, s.y0);
  }

  // ---- Typing: the device's keyboard through a hidden field ----
  function startTyping() {
    typing = true;
    input.value = '';
    input.focus();
    root.querySelector('.b[data-name="keyboard"]').classList.add('on');
  }
  function stopTyping() {
    if (!typing) return;
    typing = false;
    input.blur();
    root.querySelector('.b[data-name="keyboard"]').classList.remove('on');
    focusGame();
  }
  function typeChar(ch) {
    var code = /^[a-z]$/i.test(ch) ? 'Key' + ch.toUpperCase() : /^[0-9]$/.test(ch) ? 'Digit' + ch : ch === ' ' ? 'Space' : '';
    key(code, true, ch);
    key(code, false, ch);
  }
  // Enter sends what was typed (a chat line) and gives the game its focus back
  // at once: a game back in its world without the focus pauses itself.
  function enter() {
    stopTyping();
    key('Enter', true);
    key('Enter', false);
  }
  function wireTyping() {
    input.addEventListener('beforeinput', function (e) {
      var kind = e.inputType || '';
      if (kind === 'deleteContentBackward') { e.preventDefault(); key('Backspace', true); key('Backspace', false); return; }
      if (kind === 'insertLineBreak' || kind === 'insertParagraph') { e.preventDefault(); enter(); return; }
      if (kind.indexOf('insert') === 0 && e.data) {
        e.preventDefault();
        for (var i = 0; i < e.data.length; i++) typeChar(e.data[i]);
      }
    });
    // Some keyboards send these as plain keys.
    input.addEventListener('keydown', function (e) {
      if (e.key === 'Enter') {
        e.preventDefault();
        e.stopPropagation();
        enter();
      } else if (e.key === 'Backspace' || e.key === 'Tab' || e.key.indexOf('Arrow') === 0) {
        e.preventDefault();
        e.stopPropagation();
        key(e.key, true, e.key);
        key(e.key, false, e.key);
      } else {
        e.stopPropagation();
      }
    }, true);
    input.addEventListener('blur', function () {
      if (typing) { typing = false; root.querySelector('.b[data-name="keyboard"]').classList.remove('on'); }
    });
  }

  // ---- Mode: shown on touch, hidden by a real mouse ----
  function setTouchMode(on) {
    touchMode = on;
    root.classList.toggle('on', on && !pageScreen());
    if (!on) {
      for (var id in touches) delete touches[id];
      stickEnd();
      Object.keys(held).forEach(function (c) { hold(c, false); });
    }
    sync();
  }
  var wasWorld = null;
  function sync() {
    var world = inWorld();
    if (world !== wasWorld) {
      wasWorld = world;
      root.classList.toggle('world', world);
      root.classList.toggle('menus', !world);
      if (!world) {
        // Leaving the world lets go of what was held, but a sneak toggle stays.
        stickEnd();
        ['Space'].forEach(function (c) { hold(c, false); });
        // A button held through the change still lets go when its finger lifts.
        for (var id in touches) {
          var k = touches[id].kind;
          if (k !== 'menu' && k !== 'button') delete touches[id];
        }
      } else {
        stopTyping();
      }
      layout();
    }
  }
  // Not over the page's own screens: the zip picker and the loading status.
  function pageScreen() {
    if (document.getElementById('horizon-files')) return true;
    var status = document.getElementById('horizon-status');
    return !!status && getComputedStyle(status).display !== 'none';
  }
  function tick() {
    if (touchMode) {
      root.classList.toggle('on', !pageScreen());
      sync();
    }
    requestAnimationFrame(tick);
  }
  window.addEventListener('pointermove', function (e) {
    if (e.isTrusted && e.pointerType === 'mouse' && touchMode && (e.movementX || e.movementY)) setTouchMode(false);
  }, true);
  window.addEventListener('touchstart', function () { if (!touchMode) setTouchMode(true); }, { capture: true, passive: true });
  document.addEventListener('pointerlockchange', function () { if (touchMode) sync(); });

  function ready() {
    build();
    requestAnimationFrame(tick);
    if (navigator.maxTouchPoints > 0 && !window.matchMedia('(pointer: fine)').matches) setTouchMode(true);
  }
  if (document.body) ready();
  else document.addEventListener('DOMContentLoaded', ready);
  // For tests: the controls' state.
  window.horizonTouch = { mode: function () { return touchMode; }, world: inWorld, hotbar: hotbar };
})();
