#!/bin/bash
set -e
cd "$(dirname "$0")/../.."
PORT=9466

pkill -9 -f "obscura serve" 2>/dev/null || true
sleep 1

./target/release/obscura serve --port $PORT -v &>/tmp/obscura-verbose.log &
SERVER_PID=$!
trap "kill -9 $SERVER_PID 2>/dev/null" EXIT

for i in $(seq 1 10); do
  curl -s --max-time 1 http://127.0.0.1:$PORT/health >/dev/null 2>&1 && break
  sleep 1
done

timeout 20 node -e "
const puppeteer = require('./tests/e2e/node_modules/puppeteer-core');
(async () => {
  const browser = await puppeteer.connect({browserWSEndpoint: 'ws://127.0.0.1:$PORT/devtools/browser'});
  const page = await browser.newPage();
  
  // Try with domcontentloaded instead of load
  try {
    await page.goto('https://example.com', {waitUntil: 'domcontentloaded', timeout: 10000});
    console.log('✅ domcontentloaded works. Title:', await page.title());
  } catch(e) {
    console.log('❌ domcontentloaded fails:', e.message.substring(0,100));
  }
  
  // Try second goto on same page
  try {
    await page.goto('https://quotes.toscrape.com', {timeout: 10000});
    console.log('✅ second goto works. Title:', await page.title());
  } catch(e) {
    console.log('❌ second goto fails:', e.message.substring(0,100));
  }
  
  await browser.disconnect();
  process.exit(0);
})().catch(e => { console.error(e.message); process.exit(1); });
" 2>&1

echo "--- Server log (last 20 lines) ---"
tail -20 /tmp/obscura-verbose.log 2>/dev/null
