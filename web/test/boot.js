// Opens web/out/horizon.html in headless Chromium, prints its console and
// errors, and saves a screenshot:
//   node web/test/boot.js [seconds] [screenshot.png] [game-files.zip]
// Without a zip it starts without game files. After `seconds` it pauses the
// page in the debugger and prints where it was: a page stuck in a loop sends
// nothing else (its console messages wait for the task to end).
const { chromium } = require('playwright');
const path = require('path');
const http = require('http');
const fs = require('fs');

// Serves web/out, and takes each console line at /log as it is written: a page
// that crashes or hangs loses the console messages its task had not sent yet.
function serve(onLine) {
  const root = path.resolve(__dirname, '..', 'out');
  const server = http.createServer((req, res) => {
    if (req.method === 'POST' && req.url === '/log') {
      let body = '';
      req.on('data', (c) => (body += c));
      req.on('end', () => { onLine(body); res.writeHead(204, { 'Access-Control-Allow-Origin': '*' }); res.end(); });
      return;
    }
    const file = path.join(root, decodeURIComponent(req.url.split('?')[0]));
    if (!file.startsWith(root) || !fs.existsSync(file)) { res.writeHead(404); res.end(); return; }
    res.writeHead(200, { 'Content-Type': file.endsWith('.html') ? 'text/html' : 'application/octet-stream' });
    fs.createReadStream(file).pipe(res);
  });
  return new Promise((ok) => server.listen(0, '127.0.0.1', () => ok(server)));
}

(async () => {
  const seconds = Number(process.argv[2] || 20);
  const shot = process.argv[3] || path.join(__dirname, '..', 'out', 'boot.png');
  const zip = process.argv[4];
  const browser = await chromium.launch({ args: ['--enable-unsafe-swiftshader', '--use-angle=swiftshader', '--ignore-gpu-blocklist'].concat(process.env.CHROME_LOG ? ['--enable-logging=stderr', '--v=0'] : []) });
  const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });
  const t0 = Date.now();
  const stamp = () => ((Date.now() - t0) / 1000).toFixed(1).padStart(5);
  const server = await serve((line) => console.log(`${stamp()} ${line}`));
  const port = server.address().port;
  if (process.env.NO_FORWARD) page.on('console', (m) => console.log(`${stamp()} [${m.type()}] ${m.text()}`));
  else await page.addInitScript(() => {
    for (const kind of ['log', 'info', 'warn', 'error']) {
      const original = console[kind].bind(console);
      console[kind] = (...args) => {
        original(...args);
        try {
          const xhr = new XMLHttpRequest();
          xhr.open('POST', '/log', false);
          xhr.send('[' + kind + '] ' + args.map(String).join(' '));
        } catch (e) {}
      };
    }
  });
  page.on('pageerror', (e) => console.log(`${stamp()} [pageerror] ${e.message}`));
  page.on('crash', () => console.log(`${stamp()} [crash] the page crashed`));
  const cdp = await page.context().newCDPSession(page);
  await cdp.send('Debugger.enable');
  let paused = null;
  cdp.on('Debugger.paused', (e) => { paused = e; });

  const within = (promise, ms) => Promise.race([promise, new Promise((r) => setTimeout(r, ms))]).catch(() => {});
  const finish = async () => {
    console.log(`${stamp()} [test] pausing the page`);
    await within(cdp.send('Debugger.pause'), 20000);
    for (let i = 0; i < 80 && !paused; i++) await new Promise((r) => setTimeout(r, 250));
    if (paused) {
      const frames = paused.callFrames.map((f) => f.functionName).filter((n) => n);
      console.log(`${stamp()} [stack] busy in ${frames.length ? '' : '(idle)'}`);
      for (const n of frames.slice(0, 45)) console.log('    ' + n);
      await within(cdp.send('Debugger.resume'), 2000);
    }
    const mem = await Promise.race([
      page.evaluate(() => (performance.memory ? Math.round(performance.memory.usedJSHeapSize / 1048576) + ' MiB JS heap' : 'n/a')),
      new Promise((r) => setTimeout(() => r('n/a (busy)'), 3000)),
    ]).catch(() => 'n/a');
    console.log(`${stamp()} [memory] ${mem}`);
    await Promise.race([page.screenshot({ path: shot }), new Promise((r) => setTimeout(r, 5000))]).catch(() => {});
    console.log(`${stamp()} [screenshot] ${shot}`);
    await within(browser.close(), 5000);
    server.close();
    process.exit(0);
  };
  setTimeout(finish, seconds * 1000);

  await page.goto(`http://127.0.0.1:${port}/horizon.html`);
  await page.waitForSelector('#horizon-files', { timeout: 60000 });
  if (zip) await page.setInputFiles('#horizon-files input[type=file]', path.resolve(zip));
  else await page.click('#horizon-files button');
})();
