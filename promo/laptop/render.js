// Renders the lid animation (index.html) to a PNG sequence with headless Edge/Chrome, then
// (if ffmpeg is on PATH) encodes an MP4. Every frame is deterministic: the page exposes
// WB.setTime(cycle) and we screenshot after each call, so nothing depends on wall-clock timing.
//
//   node render.js --style silk --w 1080 --h 1920 --frames 210 --cam front --out ..\out\lid-silk.mp4
//   node render.js --img "C:\path\wallpaper.png"     (any picture as the desktop)
//
// First time: npm install (installs puppeteer-core; it drives the browser you already have).
const path = require('path');
const fs = require('fs');
const { execFileSync, spawnSync } = require('child_process');

const args = process.argv.slice(2);
const opt = (name, def) => { const i = args.indexOf('--' + name); return i >= 0 && args[i + 1] !== undefined ? args[i + 1] : def; };
const style = opt('style', 'silk');
const W = +opt('w', 1080), H = +opt('h', 1920);
const frames = +opt('frames', 210);
const fps = +opt('fps', 30);
const cam = opt('cam', 'front');
const zone = opt('zone', '50');
const shut = opt('shut', '0');
const pace = opt('pace', 'even');
const open = opt('open', '110');
const drift = opt('drift', '1');
const img = opt('img', '');
const only = opt('only', '').split(',').map(s => s.trim()).filter(Boolean).map(Number); // e.g. --only 60,72,80 for quick checks
const out = path.resolve(opt('out', path.join('..', 'out', `lid-${style}-${W}x${H}.mp4`)));
const frameDir = path.resolve(opt('frames-dir', path.join('..', 'out', `lid-frames-${style}`)));

const browsers = [
  process.env.WINBEND_BROWSER,
  'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe',
  'C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe',
  'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
  'C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe',
].filter(p => p && fs.existsSync(p));
if (!browsers.length) { console.error('no Edge/Chrome found; set WINBEND_BROWSER=path\\to\\msedge.exe'); process.exit(2); }

(async () => {
  const puppeteer = require('puppeteer-core');
  fs.mkdirSync(frameDir, { recursive: true });
  for (const f of fs.readdirSync(frameDir)) if (f.endsWith('.png')) fs.unlinkSync(path.join(frameDir, f));

  const browser = await puppeteer.launch({
    executablePath: browsers[0],
    headless: 'new',
    args: ['--ignore-gpu-blocklist', '--enable-gpu-rasterization', '--use-angle=default', '--enable-unsafe-swiftshader', `--window-size=${W},${H}`],
  });
  const page = await browser.newPage();
  await page.setViewport({ width: W, height: H, deviceScaleFactor: 1 });
  page.on('pageerror', e => console.error('page error:', e.message));
  const html = path.resolve(__dirname, 'index.html');
  const params = new URLSearchParams({ still: '1', clean: '1', w: W, h: H, style, cam, zone, shut, pace, open, drift, t: '0' });
  if (img) params.set('img', 'file:///' + path.resolve(img).replace(/\\/g, '/'));
  await page.goto('file:///' + html.replace(/\\/g, '/') + '?' + params.toString(), { waitUntil: 'load' });
  await page.waitForFunction('window.WB && window.WB.isReady === true', { timeout: 30000 });
  const canvas = await page.$('#c');
  const t0 = Date.now();
  const list = only.length ? only : Array.from({ length: frames }, (_, i) => i);
  for (const i of list) {
    const info = await page.evaluate(c => window.WB.setTime(c), i / frames);
    await canvas.screenshot({ path: path.join(frameDir, String(i).padStart(5, '0') + '.png'), omitBackground: false });
    if (only.length) console.log(`frame ${i}: lid ${info.angle.toFixed(1)}°  fold ${info.fold.toFixed(2)}`);
    else if (i % 30 === 0) process.stdout.write(`frame ${i + 1}/${frames}\n`);
  }
  await browser.close();
  console.log(`rendered ${list.length} frames in ${((Date.now() - t0) / 1000).toFixed(1)} s -> ${frameDir}`);
  if (only.length) return;

  const ff = spawnSync('ffmpeg', ['-version'], { stdio: 'ignore' });
  if (ff.error) { console.log('ffmpeg not found; frames are in', frameDir); return; }
  fs.mkdirSync(path.dirname(out), { recursive: true });
  execFileSync('ffmpeg', ['-y', '-loglevel', 'error', '-framerate', String(fps), '-i', path.join(frameDir, '%05d.png'), '-c:v', 'libx264', '-pix_fmt', 'yuv420p', '-crf', '16', '-movflags', '+faststart', out], { stdio: 'inherit' });
  console.log('wrote', out);
})().catch(e => { console.error(e); process.exit(1); });
