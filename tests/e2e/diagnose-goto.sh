#!/bin/bash
set -e
cd "$(dirname "$0")/../.."
PORT=9462

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
const WebSocket = require('./tests/e2e/node_modules/ws');
const ws = new WebSocket('ws://127.0.0.1:$PORT/devtools/browser');
let mid = 1;
let sessionId = null;
const allEvents = [];
let navigated = false;

const send = (method, params={}, sid) => {
  const id = mid++;
  const msg = {id, method, params};
  if (sid) msg.sessionId = sid;
  ws.send(JSON.stringify(msg));
  return id;
};

ws.on('open', () => {
  console.log('=== Puppeteer page.goto() flow diagnosis ===');
  send('Target.createTarget', {url: 'about:blank'});
});

ws.on('message', raw => {
  const msg = JSON.parse(raw);
  
  if (msg.method) {
    allEvents.push(msg.method);
    if (!msg.method.includes('Target.attached'))
      console.log('  EVT:', msg.method, msg.params?.name || '');
    if (msg.method === 'Page.loadEventFired') {
      console.log('\\n=== SUCCESS ===');
      ws.close(); process.exit(0);
    }
    return;
  }
  
  // Response handling
  if (msg.result?.targetId && !sessionId) {
    console.log('1. Created target:', msg.result.targetId);
    send('Target.attachToTarget', {targetId: msg.result.targetId, flatten: true});
  }
  else if (msg.result?.sessionId && !sessionId) {
    sessionId = msg.result.sessionId;
    console.log('2. Session:', sessionId);
    
    // Send Puppeteer init commands
    send('Page.enable', {}, sessionId);
    send('Page.setLifecycleEventsEnabled', {enabled: true}, sessionId);
    send('Runtime.enable', {}, sessionId);
    send('Network.enable', {}, sessionId);
    send('Audits.enable', {}, sessionId);
    
    // Navigate immediately after init
    setTimeout(() => {
      console.log('3. Navigating...');
      send('Page.navigate', {url: 'https://example.com'}, sessionId);
      navigated = true;
    }, 500);
  }
  else if (navigated && msg.result?.frameId) {
    console.log('4. Nav response, frame:', msg.result.frameId);
    console.log('5. Waiting for lifecycle events...');
  }
});

setTimeout(() => {
  console.log('\\nTIMEOUT. Events:', allEvents.filter(e => e.startsWith('Page.')).join(' -> '));
  ws.close(); process.exit(1);
}, 15000);
"
