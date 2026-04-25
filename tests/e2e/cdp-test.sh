#!/bin/bash
set -e
cd "$(dirname "$0")/../.."
PORT=9450

pkill -9 -f "obscura serve" 2>/dev/null || true
sleep 1

# Start server
./target/release/obscura serve --port $PORT &>/dev/null &
SERVER_PID=$!
trap "kill -9 $SERVER_PID 2>/dev/null" EXIT

# Wait for server
for i in $(seq 1 10); do
  if curl -s --max-time 1 http://127.0.0.1:$PORT/health >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

# Run CDP test
timeout 15 node -e "
const WebSocket = require('./tests/e2e/node_modules/ws');
const ws = new WebSocket('ws://127.0.0.1:$PORT/devtools/browser');
let mid = 1;
ws.on('error', e => { console.log('WS error:', e.message); process.exit(1); });
ws.on('open', () => {
  console.log('connected');
  ws.send(JSON.stringify({id: mid++, method: 'Target.createTarget', params: {url: 'about:blank'}}));
});
const evts = [];
ws.on('message', (raw) => {
  const m = JSON.parse(raw);
  if (m.id === 1 && m.result) {
    console.log('target:', m.result.targetId);
    ws.send(JSON.stringify({id: mid++, method: 'Target.attachToTarget', params: {targetId: m.result.targetId, flatten: true}}));
  } else if (m.id === 2 && m.result) {
    console.log('session:', m.result.sessionId);
    ws.send(JSON.stringify({id: mid++, method: 'Page.navigate', params: {url: 'https://example.com'}, sessionId: m.result.sessionId}));
  } else if (m.method) {
    evts.push(m.method);
    if (m.method === 'Page.loadEventFired') {
      console.log('SUCCESS: loadEventFired received');
      console.log('Events:', evts.join(' -> '));
      ws.close(); process.exit(0);
    }
  } else if (m.id === 3) {
    console.log('nav resp:', JSON.stringify(m.result).substring(0,80));
  }
});
setTimeout(() => { console.log('TIMEOUT. Events:', evts.join(' -> ')); ws.close(); process.exit(1); }, 12000);
"
