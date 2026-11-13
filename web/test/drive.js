// Drives web/out/horizon.html in headless Chromium through a list of steps, printing
// the console and taking screenshots on the way:
//   node web/test/drive.js game-files.zip steps.json [width] [height]
// OUT=folder serves the horizon.html in that folder instead of web/out.
// RUNNER=1 opens it through web/runner/WASM_Runner_Controller.html, as the owner does.
// steps.json is a list of steps, each one of:
//   {"wait": seconds}            {"click": [x, y]}       {"key": "Escape"}
//   {"type": "text"}             {"shot": "path.png"}    {"hold": "KeyW", "for": seconds}
//   {"move": [dx, dy]}           (relative mouse motion, as a locked pointer sends)
//   {"eval": "js expression"}    (prints its value)
//   {"profile": seconds, "top": n} (prints where the time went)
//   {"frames": seconds, "label": "x"} (frame rate and hitches)
//   {"down": "KeyW"} / {"up": "KeyW"} (holds a key across other steps)
//   with TOUCH=1: {"tap": [x, y]}, {"press": [x, y], "ms": n}, {"swipe": [[x, y], [x, y]], "ms": n},
//   {"touches": [[[x, y], [x, y]], ...], "ms": n} (several fingers at once)
// THROTTLE=4 slows the page's CPU fourfold, about an iPad's speed.
// Coordinates are CSS pixels of the page.
const { chromium } = require('playwright');
const path = require('path');
const http = require('http');
const fs = require('fs');

function serve() {
  // OUT=folder serves another build's page (the shipped one in another checkout).
  const root = path.resolve(process.env.OUT || path.join(__dirname, '..', 'out'));
  const server = http.createServer((req, res) => {
    if (req.url === '/runner.html') {
      res.writeHead(200, { 'Content-Type': 'text/html' });
      fs.createReadStream(path.join(__dirname, '..', 'runner', 'WASM_Runner_Controller.html')).pipe(res);
      return;
    }
    const file = path.join(root, decodeURIComponent(req.url.split('?')[0]));
    if (!file.startsWith(root) || !fs.existsSync(file)) { res.writeHead(404); res.end(); return; }
    res.writeHead(200, { 'Content-Type': file.endsWith('.html') ? 'text/html' : 'application/octet-stream' });
    fs.createReadStream(file).pipe(res);
  });
  return new Promise((ok) => server.listen(0, '127.0.0.1', () => ok(server)));
}

// Samples the page's JavaScript and wasm for `seconds` and prints the functions
// that took the most time, by their own time and with what they called.
async function profile(page, seconds, top) {
  const cdp = await page.context().newCDPSession(page);
  await cdp.send('Profiler.enable');
  await cdp.send('Profiler.setSamplingInterval', { interval: 500 });
  await cdp.send('Profiler.start');
  await page.waitForTimeout(seconds * 1000);
  const { profile: p } = await cdp.send('Profiler.stop');
  const byId = new Map(p.nodes.map((n) => [n.id, n]));
  const self = new Map(), total = new Map(), parent = new Map();
  for (const n of p.nodes) for (const c of n.children || []) parent.set(c, n.id);
  const counts = new Map();
  for (const id of p.samples) counts.set(id, (counts.get(id) || 0) + 1);
  const name = (n) => (n.callFrame.functionName || '(anonymous)').replace(/\[[0-9a-f]{16}\]/g, '').slice(0, 140);
  for (const [id, c] of counts) {
    const n = byId.get(id);
    self.set(name(n), (self.get(name(n)) || 0) + c);
    const seen = new Set();
    for (let at = id; at !== undefined; at = parent.get(at)) {
      const nm = name(byId.get(at));
      if (seen.has(nm)) continue;
      seen.add(nm);
      total.set(nm, (total.get(nm) || 0) + c);
    }
  }
  const all = p.samples.length;
  const show = (m, title) => {
    console.log(`[profile] ${title} (${all} samples)`);
    [...m].sort((a, b) => b[1] - a[1]).slice(0, top)
      .forEach(([n, c]) => console.log(`  ${(100 * c / all).toFixed(1).padStart(5)}%  ${n}`));
  };
  show(self, 'own time');
  show(new Map([...total].filter(([n]) => /horizon|wgpu|winit|naga/.test(n))), 'with callees (game functions)');
}

// Fingers down at `from`, moving to `to` over `ms`, then up: real touch events
// (Input.dispatchTouchEvent), several fingers at once when given several.
async function touch(page, fingers, ms) {
  const cdp = await page.context().newCDPSession(page);
  const at = (f, k) => ({ x: f.from[0] + (f.to[0] - f.from[0]) * k, y: f.from[1] + (f.to[1] - f.from[1]) * k });
  const points = (k) => fingers.map((f, i) => ({ ...at(f, k), id: i + 1, radiusX: 8, radiusY: 8, force: 1 }));
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: points(0) });
  const steps = Math.max(1, Math.round(ms / 16));
  for (let i = 1; i <= steps; i++) {
    await new Promise((r) => setTimeout(r, ms / steps));
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: points(i / steps) });
  }
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await cdp.detach();
}

(async () => {
  const [zip, stepsFile, w, h] = process.argv.slice(2);
  const steps = JSON.parse(fs.readFileSync(stepsFile, 'utf8'));
  const width = Number(w || 1180), height = Number(h || 820);
  // GPU=1 draws with the computer's graphics card (headless, through Direct3D), as a player's browser does;
  // otherwise Chromium's software renderer, which is slow and draws some things differently.
  const gpu = !!process.env.GPU;
  const browser = await chromium.launch(gpu
    ? { args: ['--use-angle=d3d11', '--enable-gpu', '--ignore-gpu-blocklist', '--autoplay-policy=no-user-gesture-required'] }
    : { args: ['--enable-unsafe-swiftshader', '--use-angle=swiftshader', '--ignore-gpu-blocklist'] });
  // TOUCH=1: a touch screen, as an iPad's (the touch steps below need it).
  const page = await browser.newPage({ viewport: { width, height }, hasTouch: !!process.env.TOUCH });
  // THROTTLE=n runs the page's CPU n times slower: about an iPad's speed at 4.
  if (process.env.THROTTLE) {
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('Emulation.setCPUThrottlingRate', { rate: Number(process.env.THROTTLE) });
  }
  const t0 = Date.now();
  const stamp = () => ((Date.now() - t0) / 1000).toFixed(1).padStart(6);
  page.on('console', (m) => console.log(`${stamp()} [${m.type()}] ${m.text()}`));
  page.on('pageerror', (e) => console.log(`${stamp()} [pageerror] ${e.message}`));
  page.on('crash', () => console.log(`${stamp()} [crash] the page crashed`));
  const server = await serve();
  if (process.env.RUNNER) {
    // Through the owner's runner: it reads the picked game file as text and writes it in.
    await page.goto(`http://127.0.0.1:${server.address().port}/runner.html`);
    await page.setInputFiles('#file-input', path.join(path.resolve(process.env.OUT || path.join(__dirname, '..', 'out')), 'horizon.html'));
  } else {
    await page.goto(`http://127.0.0.1:${server.address().port}/horizon.html`);
  }
  await page.waitForSelector('#horizon-files', { timeout: 120000 });
  await page.setInputFiles('#horizon-files input[type=file]', path.resolve(zip));
  for (const s of steps) {
    console.log(`${stamp()} [step] ${JSON.stringify(s)}`);
    if (s.wait) await page.waitForTimeout(s.wait * 1000);
    else if (s.click) { await page.mouse.move(s.click[0], s.click[1]); await page.mouse.down(); await page.waitForTimeout(80); await page.mouse.up(); }
    else if (s.key) await page.keyboard.press(s.key);
    else if (s.tap) await touch(page, [{ from: s.tap, to: s.tap }], 60);
    else if (s.press) await touch(page, [{ from: s.press, to: s.press }], s.ms || 600);
    else if (s.swipe) await touch(page, [{ from: s.swipe[0], to: s.swipe[1] }], s.ms || 400);
    else if (s.touches) await touch(page, s.touches.map((t) => ({ from: t[0], to: t[1] || t[0] })), s.ms || 600);
    else if (s.down) await page.keyboard.down(s.down);
    else if (s.up) await page.keyboard.up(s.up);
    else if (s.profile) await profile(page, s.profile, s.top || 30);
    else if (s.frames) {
      // Frame times seen by requestAnimationFrame for `frames` seconds: the
      // frame rate and the hitches a player feels.
      const r = await page.evaluate((secs) => new Promise((ok) => {
        const times = []; let last = performance.now(); const end = last + secs * 1000;
        function tick(now) { times.push(now - last); last = now; if (now < end) requestAnimationFrame(tick); else ok(times); }
        requestAnimationFrame(tick);
      }), s.frames);
      const sorted = [...r].sort((a, b) => a - b), q = (f) => sorted[Math.min(sorted.length - 1, Math.floor(f * sorted.length))];
      const total = r.reduce((a, b) => a + b, 0);
      console.log(`${stamp()} [frames] ${s.label || ''} ${(1000 * r.length / total).toFixed(1)} fps, median ${q(0.5).toFixed(1)} ms, 95% ${q(0.95).toFixed(1)} ms, worst ${sorted[sorted.length - 1].toFixed(0)} ms, ${r.filter((t) => t > 50).length} over 50 ms, ${r.filter((t) => t > 100).length} over 100 ms`);
    }
    else if (s.eval) console.log(`${stamp()} [eval] ${JSON.stringify(await page.evaluate(s.eval).catch((e) => String(e)))}`);
    else if (s.type) await page.keyboard.type(s.type, { delay: 40 });
    else if (s.hold) { await page.keyboard.down(s.hold); await page.waitForTimeout((s.for || 1) * 1000); await page.keyboard.up(s.hold); }
    else if (s.move) await page.evaluate(([dx, dy]) => {
      const c = document.querySelector('canvas');
      c.dispatchEvent(new MouseEvent('mousemove', { bubbles: true, movementX: dx, movementY: dy }));
    }, s.move);
    else if (s.shot) {
      await Promise.race([page.screenshot({ path: s.shot }), new Promise((r) => setTimeout(r, 10000))]).catch(() => {});
      const size = await page.evaluate(() => {
        const c = document.querySelector('canvas'); const r = c.getBoundingClientRect();
        return `canvas css ${r.width}x${r.height} at ${r.left},${r.top}, buffer ${c.width}x${c.height}, page ${innerWidth}x${innerHeight}`;
      }).catch(() => 'n/a');
      console.log(`${stamp()} [shot] ${s.shot} (${size})`);
    }
  }
  await browser.close();
  server.close();
  process.exit(0);
})();
