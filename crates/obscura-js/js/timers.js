let _tid = 0;
const _pendingTimers = new Map();
const _clearedTimers = new Set();

globalThis.setTimeout = (fn, delay = 0, ...args) => {
  if (typeof fn !== "function") return ++_tid;
  const id = ++_tid;
  _pendingTimers.set(id, { fn, args, delay });
  const run = () => {
    if (!_clearedTimers.has(id) && _pendingTimers.has(id)) {
      _pendingTimers.delete(id);
      try { fn(...args); } catch(e) { console.error("Timer error:", e); }
    }
  };
  if (delay <= 0) {
    Promise.resolve().then(run);
  } else {
    // Schedule via microtask chain to approximate delay
    const start = Date.now();
    const check = () => {
      if (_clearedTimers.has(id)) return;
      if (Date.now() - start >= delay) { run(); }
      else { Promise.resolve().then(check); }
    };
    Promise.resolve().then(check);
  }
  return id;
};

globalThis.clearTimeout = (id) => { _clearedTimers.add(id); _pendingTimers.delete(id); };
globalThis.setInterval = (fn, delay, ...args) => {
  if (typeof fn !== "function") return ++_tid;
  const id = ++_tid;
  const tick = () => {
    if (_clearedTimers.has(id)) return;
    try { fn(...args); } catch(e) { console.error("Timer error:", e); }
    if (!_clearedTimers.has(id)) {
      _pendingTimers.set(id, { fn, args, delay });
      setTimeout(tick, delay);
    }
  };
  _pendingTimers.set(id, { fn, args, delay });
  setTimeout(tick, delay);
  return id;
};
globalThis.clearInterval = globalThis.clearTimeout;
globalThis.requestAnimationFrame = (fn) => setTimeout(fn, 0);
globalThis.cancelAnimationFrame = globalThis.clearTimeout;
globalThis.queueMicrotask = globalThis.queueMicrotask || ((fn) => Promise.resolve().then(fn));

class MessageChannel {
  constructor() {
    this.port1 = { onmessage: null, postMessage: () => {}, close() {}, addEventListener() {}, removeEventListener() {} };
    this.port2 = { onmessage: null, postMessage: () => {}, close() {}, addEventListener() {}, removeEventListener() {} };
    this.port1.postMessage = (data) => {
      Promise.resolve().then(() => { if (this.port2.onmessage) this.port2.onmessage({ data }); });
    };
    this.port2.postMessage = (data) => {
      Promise.resolve().then(() => { if (this.port1.onmessage) this.port1.onmessage({ data }); });
    };
  }
}
globalThis.MessageChannel = MessageChannel;
globalThis.MessagePort = class MessagePort { constructor(){} postMessage(){} close(){} addEventListener(){} removeEventListener(){} };

