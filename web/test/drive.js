// Drives web/out/horizon.html in headless Chromium through a list of steps, printing
// the console and taking screenshots on the way:
//   node web/test/drive.js game-files.zip steps.json [width] [height]
// OUT=folder serves the horizon.html in that folder instead of web/out.
// steps.json is a list of steps, each one of:
//   {"wait": seconds}            {"click": [x, y]}       {"key": "Escape"}
//   {"type": "text"}             {"shot": "path.png"}    {"hold": "KeyW", "for": seconds}
//   {"move": [dx, dy]}           (relative mouse motion, as a locked pointer sends)
//   {"eval": "js expression"}    (prints its value)
//   {"profile": seconds, "top": n} (prints where the time went)
// Coordinates are CSS pixels of the page.
const { chromium } = require('playwright');
const path = require('path');
const http = require('http');
const fs = require('fs');

function serve() {
  // OUT=folder serves another build's page (the shipped one in another checkout).
  const root = path.resolve(process.env.OUT || path.join(__dirname, '..', 'out'));
  const server = http.createServer((req, res) => {
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
  const page = await browser.newPage({ viewport: { width, height } });
  const t0 = Date.now();
  const stamp = () => ((Date.now() - t0) / 1000).toFixed(1).padStart(6);
  page.on('console', (m) => console.log(`${stamp()} [${m.type()}] ${m.text()}`));
  page.on('pageerror', (e) => console.log(`${stamp()} [pageerror] ${e.message}`));
  page.on('crash', () => console.log(`${stamp()} [crash] the page crashed`));
  const server = await serve();
  await page.goto(`http://127.0.0.1:${server.address().port}/horizon.html`);
  await page.waitForSelector('#horizon-files', { timeout: 120000 });
  await page.setInputFiles('#horizon-files input[type=file]', path.resolve(zip));
  for (const s of steps) {
    console.log(`${stamp()} [step] ${JSON.stringify(s)}`);
    if (s.wait) await page.waitForTimeout(s.wait * 1000);
    else if (s.click) { await page.mouse.move(s.click[0], s.click[1]); await page.mouse.down(); await page.waitForTimeout(80); await page.mouse.up(); }
    else if (s.key) await page.keyboard.press(s.key);
    else if (s.profile) await profile(page, s.profile, s.top || 30);
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
