#!/bin/bash
set -e
cd "$(dirname "$0")/../.."
PORT=9465

pkill -9 -f "obscura serve" 2>/dev/null || true
sleep 1

./target/release/obscura serve --port $PORT &>/dev/null &
SERVER_PID=$!
trap "kill -9 $SERVER_PID 2>/dev/null" EXIT

for i in $(seq 1 10); do
  curl -s --max-time 1 http://127.0.0.1:$PORT/health >/dev/null 2>&1 && break
  sleep 1
done

timeout 30 node -e "
const puppeteer = require('./tests/e2e/node_modules/puppeteer-core');
(async () => {
  const browser = await puppeteer.connect({browserWSEndpoint: 'ws://127.0.0.1:$PORT/devtools/browser'});
  const page = await browser.newPage();
  
  // Small delay to let init commands settle
  await new Promise(r => setTimeout(r, 200));
  
  try {
    await page.goto('https://example.com', {timeout: 10000});
    console.log('✅ goto works with 200ms delay. Title:', await page.title());
  } catch(e) {
    console.log('❌ still fails:', e.message.substring(0,100));
  }
  await browser.disconnect();
  process.exit(0);
})().catch(e => { console.error(e.message); process.exit(1); });
"
