// Opens web/out/horizon.html in headless Chromium, prints its console and
// errors, and saves a screenshot: node web/test/boot.js [seconds] [screenshot.png] [game-files.zip]
// Without a zip it starts without game files.
const { chromium } = require('playwright');
const path = require('path');

(async () => {
  const seconds = Number(process.argv[2] || 20);
  const shot = process.argv[3] || path.join(__dirname, '..', 'out', 'boot.png');
  const browser = await chromium.launch({ args: ['--enable-unsafe-swiftshader', '--use-angle=swiftshader', '--ignore-gpu-blocklist'] });
  const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });
  const t0 = Date.now();
  const stamp = () => ((Date.now() - t0) / 1000).toFixed(1).padStart(5);
  page.on('console', (m) => console.log(`${stamp()} [${m.type()}] ${m.text()}`));
  page.on('pageerror', (e) => console.log(`${stamp()} [pageerror] ${e.message}`));
  page.on('crash', () => console.log(`${stamp()} [crash] the page crashed`));
  const zip = process.argv[4];
  await page.goto('file://' + path.resolve(__dirname, '..', 'out', 'horizon.html'));
  await page.waitForSelector('#horizon-files', { timeout: 60000 });
  if (zip) await page.setInputFiles('#horizon-files input[type=file]', path.resolve(zip));
  else await page.click('#horizon-files button');
  await page.waitForTimeout(seconds * 1000);
  const mem = await page.evaluate(() => performance.memory ? Math.round(performance.memory.usedJSHeapSize / 1048576) + ' MiB JS heap' : 'n/a').catch(() => 'n/a');
  console.log(`${stamp()} [memory] ${mem}`);
  await page.screenshot({ path: shot });
  console.log(`${stamp()} [screenshot] ${shot}`);
  await browser.close();
})();
