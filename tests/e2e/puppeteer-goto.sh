#!/bin/bash
set -e
cd "$(dirname "$0")/../.."
PORT=9464

pkill -9 -f "obscura serve" 2>/dev/null || true
sleep 1

./target/release/obscura serve --port $PORT &>/dev/null &
SERVER_PID=$!
trap "kill -9 $SERVER_PID 2>/dev/null" EXIT

for i in $(seq 1 10); do
  curl -s --max-time 1 http://127.0.0.1:$PORT/health >/dev/null 2>&1 && break
  sleep 1
done

timeout 60 node -e "
const puppeteer = require('./tests/e2e/node_modules/puppeteer-core');
let passed = 0, failed = 0;

async function test(name, fn) {
  try { await fn(); passed++; console.log('✅', name); }
  catch(e) { failed++; console.log('❌', name + ':', e.message.substring(0,100)); }
}

(async () => {
  const browser = await puppeteer.connect({browserWSEndpoint: 'ws://127.0.0.1:$PORT/devtools/browser'});

  await test('page.goto example.com', async () => {
    const page = await browser.newPage();
    await page.goto('https://example.com', {timeout: 15000});
    const title = await page.title();
    if (title !== 'Example Domain') throw new Error('title: ' + title);
    await page.close();
  });

  await test('page.goto quotes.toscrape.com', async () => {
    const page = await browser.newPage();
    await page.goto('https://quotes.toscrape.com', {timeout: 15000});
    const quotes = await page.evaluate(() => document.querySelectorAll('.quote').length);
    if (quotes !== 10) throw new Error('quotes: ' + quotes);
    await page.close();
  });

  await test('page.goto HN + evaluate', async () => {
    const page = await browser.newPage();
    await page.goto('https://news.ycombinator.com', {timeout: 15000});
    const stories = await page.evaluate(() =>
      Array.from(document.querySelectorAll('.titleline > a')).map(a => a.textContent)
    );
    if (stories.length < 20) throw new Error('stories: ' + stories.length);
    await page.close();
  });

  await test('page.evaluate complex', async () => {
    const page = await browser.newPage();
    await page.goto('https://example.com', {timeout: 15000});
    const result = await page.evaluate(() => ({
      title: document.title,
      h1: document.querySelector('h1').textContent,
      links: document.querySelectorAll('a').length,
      hasBody: !!document.body,
    }));
    if (result.title !== 'Example Domain') throw new Error('title: ' + result.title);
    if (result.links !== 1) throw new Error('links: ' + result.links);
    await page.close();
  });

  await test('page.content()', async () => {
    const page = await browser.newPage();
    await page.goto('https://example.com', {timeout: 15000});
    const html = await page.content();
    if (!html.includes('Example Domain')) throw new Error('missing title in HTML');
    if (!html.includes('<h1>')) throw new Error('missing h1 in HTML');
    await page.close();
  });

  await test('page.setUserAgent', async () => {
    const page = await browser.newPage();
    await page.setUserAgent('ObscuraTest/1.0');
    await page.goto('https://httpbin.org/user-agent', {timeout: 15000});
    const body = await page.evaluate(() => document.body.textContent);
    if (!body.includes('ObscuraTest/1.0')) throw new Error('UA not set: ' + body);
    await page.close();
  });

  await test('navigator properties', async () => {
    const page = await browser.newPage();
    await page.goto('https://example.com', {timeout: 15000});
    const nav = await page.evaluate(() => ({
      webdriver: navigator.webdriver,
      platform: navigator.platform,
      chrome: typeof chrome !== 'undefined',
    }));
    if (nav.webdriver !== undefined) throw new Error('webdriver should be undefined');
    if (nav.platform !== 'Linux x86_64') throw new Error('platform: ' + nav.platform);
    await page.close();
  });

  await browser.disconnect();
  console.log('\nResults:', passed, 'passed,', failed, 'failed');
  process.exit(failed > 0 ? 1 : 0);
})().catch(e => { console.error('FATAL:', e.message); process.exit(1); });
"
