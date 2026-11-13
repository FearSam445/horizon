// Drives web/out/horizon.html in headless Chromium through a list of steps, printing
// the console and taking screenshots on the way:
//   node web/test/drive.js game-files.zip steps.json [width] [height]
// steps.json is a list of steps, each one of:
//   {"wait": seconds}            {"click": [x, y]}       {"key": "Escape"}
//   {"type": "text"}             {"shot": "path.png"}    {"hold": "KeyW", "for": seconds}
//   {"move": [dx, dy]}           (relative mouse motion, as a locked pointer sends)
// Coordinates are CSS pixels of the page.
const { chromium } = require('playwright');
const path = require('path');
const http = require('http');
const fs = require('fs');

function serve() {
  const root = path.resolve(__dirname, '..', 'out');
  const server = http.createServer((req, res) => {
    const file = path.join(root, decodeURIComponent(req.url.split('?')[0]));
    if (!file.startsWith(root) || !fs.existsSync(file)) { res.writeHead(404); res.end(); return; }
    res.writeHead(200, { 'Content-Type': file.endsWith('.html') ? 'text/html' : 'application/octet-stream' });
    fs.createReadStream(file).pipe(res);
  });
  return new Promise((ok) => server.listen(0, '127.0.0.1', () => ok(server)));
}

(async () => {
  const [zip, stepsFile, w, h] = process.argv.slice(2);
  const steps = JSON.parse(fs.readFileSync(stepsFile, 'utf8'));
  const width = Number(w || 1180), height = Number(h || 820);
  // GPU=1 draws with the computer's graphics card (headless, through Direct3D), as a player's browser does;
  // otherwise Chromium's software renderer, which is slow and draws some things differently.
  const gpu = !!process.env.GPU;
  const browser = await chromium.launch(gpu
    ? { args: ['--use-angle=d3d11', '--enable-gpu', '--ignore-gpu-blocklist'] }
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
