#!/bin/bash
set -e
cd "$(dirname "$0")/../.."
PORT=9463

pkill -9 -f "obscura serve" 2>/dev/null || true
sleep 1

./target/release/obscura serve --port $PORT &>/dev/null &
SERVER_PID=$!
trap "kill -9 $SERVER_PID 2>/dev/null" EXIT

for i in $(seq 1 10); do
  curl -s --max-time 1 http://127.0.0.1:$PORT/health >/dev/null 2>&1 && break
  sleep 1
done

timeout 20 node -e "
// Intercept WebSocket to log ALL CDP traffic
const RealWS = require('./tests/e2e/node_modules/ws');
const msgs = [];

class SpyWS extends RealWS {
  constructor(...args) {
    super(...args);
    const self = this;
    this.on('message', d => {
      const m = JSON.parse(d);
      msgs.push({dir:'<-', ...m});
    });
    const origSend = this.send.bind(this);
    this.send = (data, opts, cb) => {
      try { const m = JSON.parse(data); msgs.push({dir:'->', ...m}); } catch{}
      return origSend(data, opts, cb);
    };
  }
}

// Replace ws in require cache
const wsPath = require.resolve('./tests/e2e/node_modules/ws');
require.cache[wsPath] = {id: wsPath, filename: wsPath, loaded: true, exports: SpyWS};

const puppeteer = require('./tests/e2e/node_modules/puppeteer-core');

(async () => {
  const browser = await puppeteer.connect({browserWSEndpoint: 'ws://127.0.0.1:$PORT/devtools/browser'});
  
  console.log('=== Messages during connect ===');
  msgs.forEach(m => {
    if (m.dir === '->') console.log('->', m.method, m.sessionId ? '(session)' : '');
    else if (m.method) console.log('<- EVT', m.method);
    else if (m.error) console.log('<- ERR', m.id, m.error.message.substring(0,60));
    else console.log('<- OK', m.id);
  });
  
  msgs.length = 0;
  console.log('\\n=== Messages during newPage() ===');
  const page = await browser.newPage();
  msgs.forEach(m => {
    if (m.dir === '->') console.log('->', m.method, m.sessionId ? '(session)' : '');
    else if (m.method) console.log('<- EVT', m.method);
    else if (m.error) console.log('<- ERR', m.id, m.error.message.substring(0,60));
    else console.log('<- OK', m.id);
  });
  
  msgs.length = 0;
  console.log('\\n=== Messages during goto() ===');
  try {
    await page.goto('https://example.com', {timeout: 10000});
    console.log('GOTO SUCCESS');
  } catch(e) {
    console.log('GOTO FAIL:', e.message.substring(0,100));
  }
  msgs.forEach(m => {
    if (m.dir === '->') console.log('->', m.method, m.sessionId ? '(session)' : '');
    else if (m.method) console.log('<- EVT', m.method);
    else if (m.error) console.log('<- ERR', m.id, m.error.message.substring(0,60));
    else console.log('<- OK', m.id);
  });
  
  await browser.disconnect();
  process.exit(0);
})().catch(e => { console.error('FATAL:', e.message); process.exit(1); });
"
