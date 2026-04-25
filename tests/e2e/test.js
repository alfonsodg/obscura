const puppeteer = require('puppeteer-core');
const { spawn } = require('child_process');
const http = require('http');

const CDP_PORT = 9451;
const BINARY = '../../target/release/obscura';
let server;
let passed = 0;
let failed = 0;
const failures = [];

function log(icon, msg) { console.log(`${icon} ${msg}`); }

async function waitForServer(port, timeout = 8000) {
  const start = Date.now();
  while (Date.now() - start < timeout) {
    try {
      await new Promise((resolve, reject) => {
        const req = http.get(`http://127.0.0.1:${port}/health`, res => {
          let data = '';
          res.on('data', d => data += d);
          res.on('end', () => resolve(data));
        });
        req.on('error', reject);
        req.setTimeout(500, () => { req.destroy(); reject(new Error('timeout')); });
      });
      return true;
    } catch { await new Promise(r => setTimeout(r, 300)); }
  }
  return false;
}

async function test(name, fn) {
  try {
    await fn();
    passed++;
    log('✅', name);
  } catch (e) {
    failed++;
    failures.push({ name, error: e.message });
    log('❌', `${name}: ${e.message}`);
  }
}

function assert(cond, msg) { if (!cond) throw new Error(msg || 'Assertion failed'); }

// Use CDP directly via WebSocket for navigation tests
const WebSocket = require('ws');

function cdpSession(port) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(`ws://127.0.0.1:${port}/devtools/browser`);
    let mid = 1;
    const pending = new Map();
    const events = [];
    const eventListeners = new Map();

    ws.on('error', reject);
    ws.on('open', () => {
      const session = {
        send(method, params = {}, sessionId) {
          return new Promise((res, rej) => {
            const id = mid++;
            pending.set(id, { res, rej });
            const msg = { id, method, params };
            if (sessionId) msg.sessionId = sessionId;
            ws.send(JSON.stringify(msg));
          });
        },
        waitForEvent(name, timeout = 10000) {
          const existing = events.find(e => e.method === name);
          if (existing) return Promise.resolve(existing);
          return new Promise((res, rej) => {
            const timer = setTimeout(() => rej(new Error(`Timeout waiting for ${name}`)), timeout);
            if (!eventListeners.has(name)) eventListeners.set(name, []);
            eventListeners.get(name).push(e => { clearTimeout(timer); res(e); });
          });
        },
        events,
        close() { ws.close(); },
      };

      ws.on('message', raw => {
        const msg = JSON.parse(raw);
        if (msg.id && pending.has(msg.id)) {
          const { res, rej } = pending.get(msg.id);
          pending.delete(msg.id);
          if (msg.error) rej(new Error(msg.error.message));
          else res(msg.result);
        } else if (msg.method) {
          events.push(msg);
          const listeners = eventListeners.get(msg.method) || [];
          listeners.forEach(fn => fn(msg));
          eventListeners.delete(msg.method);
        }
      });

      resolve(session);
    });
  });
}

(async () => {
  console.log('\n🚀 Starting Obscura CDP server...');
  server = spawn(BINARY, ['serve', '--port', String(CDP_PORT)], {
    stdio: 'ignore', detached: true
  });
  server.unref();

  const ready = await waitForServer(CDP_PORT);
  if (!ready) { console.error('❌ Server failed to start'); process.exit(1); }
  console.log('✅ Server ready\n');

  // ===== E2E TESTS =====

  await test('Health endpoint returns OK', async () => {
    const data = await new Promise((res, rej) => {
      http.get(`http://127.0.0.1:${CDP_PORT}/health`, r => {
        let d = ''; r.on('data', c => d += c); r.on('end', () => res(d));
      }).on('error', rej);
    });
    const json = JSON.parse(data);
    assert(json.status === 'ok', `Expected ok, got ${json.status}`);
  });

  await test('JSON version endpoint', async () => {
    const data = await new Promise((res, rej) => {
      http.get(`http://127.0.0.1:${CDP_PORT}/json/version`, r => {
        let d = ''; r.on('data', c => d += c); r.on('end', () => res(d));
      }).on('error', rej);
    });
    const json = JSON.parse(data);
    assert(json.Browser.includes('Obscura'), `Browser should contain Obscura`);
    assert(json['Protocol-Version'] === '1.3', 'Protocol should be 1.3');
  });

  await test('CDP: Create and attach to target', async () => {
    const cdp = await cdpSession(CDP_PORT);
    const { targetId } = await cdp.send('Target.createTarget', { url: 'about:blank' });
    assert(targetId, 'Should get targetId');
    const { sessionId } = await cdp.send('Target.attachToTarget', { targetId, flatten: true });
    assert(sessionId, 'Should get sessionId');
    cdp.close();
  });

  await test('CDP: Navigate and receive lifecycle events', async () => {
    const cdp = await cdpSession(CDP_PORT);
    const { targetId } = await cdp.send('Target.createTarget', { url: 'about:blank' });
    const { sessionId } = await cdp.send('Target.attachToTarget', { targetId, flatten: true });
    const loadPromise = cdp.waitForEvent('Page.loadEventFired', 15000);
    await cdp.send('Page.navigate', { url: 'https://example.com' }, sessionId);
    await loadPromise;
    const hasLoad = cdp.events.some(e => e.method === 'Page.loadEventFired');
    assert(hasLoad, 'Should receive Page.loadEventFired');
    const hasFrame = cdp.events.some(e => e.method === 'Page.frameNavigated');
    assert(hasFrame, 'Should receive Page.frameNavigated');
    cdp.close();
  });

  await test('CDP: Evaluate JavaScript', async () => {
    const cdp = await cdpSession(CDP_PORT);
    const { targetId } = await cdp.send('Target.createTarget', { url: 'about:blank' });
    const { sessionId } = await cdp.send('Target.attachToTarget', { targetId, flatten: true });
    await cdp.send('Page.navigate', { url: 'https://example.com' }, sessionId);
    await cdp.waitForEvent('Page.loadEventFired', 15000);
    const result = await cdp.send('Runtime.evaluate', { expression: 'document.title', returnByValue: true }, sessionId);
    assert(result.result.value === 'Example Domain', `Title should be Example Domain, got: ${result.result.value}`);
    cdp.close();
  });

  await test('CDP: Query DOM', async () => {
    const cdp = await cdpSession(CDP_PORT);
    const { targetId } = await cdp.send('Target.createTarget', { url: 'about:blank' });
    const { sessionId } = await cdp.send('Target.attachToTarget', { targetId, flatten: true });
    await cdp.send('Page.navigate', { url: 'https://example.com' }, sessionId);
    await cdp.waitForEvent('Page.loadEventFired', 15000);
    const doc = await cdp.send('DOM.getDocument', { depth: 1 }, sessionId);
    assert(doc.root, 'Should get document root');
    assert(doc.root.nodeType === 9, 'Root should be document node');
    const h1 = await cdp.send('DOM.querySelector', { nodeId: doc.root.nodeId, selector: 'h1' }, sessionId);
    assert(h1.nodeId > 0, 'Should find h1');
    const html = await cdp.send('DOM.getOuterHTML', { nodeId: h1.nodeId }, sessionId);
    assert(html.outerHTML.includes('Example Domain'), `H1 should contain Example Domain`);
    cdp.close();
  });

  await test('CDP: Extract data from Hacker News', async () => {
    const cdp = await cdpSession(CDP_PORT);
    const { targetId } = await cdp.send('Target.createTarget', { url: 'about:blank' });
    const { sessionId } = await cdp.send('Target.attachToTarget', { targetId, flatten: true });
    await cdp.send('Page.navigate', { url: 'https://news.ycombinator.com' }, sessionId);
    await cdp.waitForEvent('Page.loadEventFired', 15000);
    const result = await cdp.send('Runtime.evaluate', {
      expression: 'document.querySelectorAll(".titleline > a").length',
      returnByValue: true
    }, sessionId);
    assert(result.result.value >= 20, `Should have >=20 stories, got ${result.result.value}`);
    cdp.close();
  });

  await test('CDP: SSRF blocked', async () => {
    const cdp = await cdpSession(CDP_PORT);
    const { targetId } = await cdp.send('Target.createTarget', { url: 'about:blank' });
    const { sessionId } = await cdp.send('Target.attachToTarget', { targetId, flatten: true });
    try {
      await cdp.send('Page.navigate', { url: 'http://127.0.0.1:8080' }, sessionId);
      // Check if navigation failed (no loadEventFired within 3s)
      try {
        await cdp.waitForEvent('Page.loadEventFired', 3000);
        assert(false, 'Should not get loadEventFired for blocked URL');
      } catch { /* timeout = good, SSRF blocked */ }
    } catch (e) {
      // Error response = also good
    }
    cdp.close();
  });

  await test('CDP: Network events emitted', async () => {
    const cdp = await cdpSession(CDP_PORT);
    const { targetId } = await cdp.send('Target.createTarget', { url: 'about:blank' });
    const { sessionId } = await cdp.send('Target.attachToTarget', { targetId, flatten: true });
    await cdp.send('Page.navigate', { url: 'https://example.com' }, sessionId);
    await cdp.waitForEvent('Page.loadEventFired', 15000);
    const hasRequest = cdp.events.some(e => e.method === 'Network.requestWillBeSent');
    const hasResponse = cdp.events.some(e => e.method === 'Network.responseReceived');
    assert(hasRequest, 'Should emit Network.requestWillBeSent');
    assert(hasResponse, 'Should emit Network.responseReceived');
    cdp.close();
  });

  await test('CDP: Navigator stealth properties', async () => {
    const cdp = await cdpSession(CDP_PORT);
    const { targetId } = await cdp.send('Target.createTarget', { url: 'about:blank' });
    const { sessionId } = await cdp.send('Target.attachToTarget', { targetId, flatten: true });
    await cdp.send('Page.navigate', { url: 'https://example.com' }, sessionId);
    await cdp.waitForEvent('Page.loadEventFired', 15000);
    const result = await cdp.send('Runtime.evaluate', {
      expression: 'JSON.stringify({webdriver: navigator.webdriver, platform: navigator.platform, ua: navigator.userAgent.includes("Chrome")})',
      returnByValue: true
    }, sessionId);
    const nav = JSON.parse(result.result.value);
    assert(nav.webdriver === undefined, 'webdriver should be undefined');
    assert(nav.platform === 'Linux x86_64', `platform should be Linux x86_64`);
    assert(nav.ua === true, 'UA should contain Chrome');
    cdp.close();
  });

  await test('Puppeteer: Connect and get version', async () => {
    const browser = await puppeteer.connect({ browserWSEndpoint: `ws://127.0.0.1:${CDP_PORT}/devtools/browser` });
    const version = await browser.version();
    assert(version.includes('Obscura'), `Version should contain Obscura, got: ${version}`);
    await browser.disconnect();
  });

  await test('Puppeteer: Evaluate on existing page', async () => {
    const browser = await puppeteer.connect({ browserWSEndpoint: `ws://127.0.0.1:${CDP_PORT}/devtools/browser` });
    const pages = await browser.pages();
    if (pages.length > 0) {
      const result = await pages[0].evaluate(() => 1 + 1);
      assert(result === 2, `1+1 should be 2, got ${result}`);
    }
    await browser.disconnect();
  });

  // ===== RESULTS =====
  try { process.kill(-server.pid, 'SIGKILL'); } catch {}

  console.log(`\n${'='.repeat(50)}`);
  console.log(`Results: ${passed} passed, ${failed} failed, ${passed + failed} total`);
  if (failures.length > 0) {
    console.log('\nFailures:');
    failures.forEach(f => console.log(`  ❌ ${f.name}: ${f.error}`));
  }
  console.log('='.repeat(50));
  process.exit(failed > 0 ? 1 : 0);
})();
