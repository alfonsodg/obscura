globalThis.__fetchInterceptEnabled = false;
globalThis.__fetchInterceptCallback = null; // Set by CDP to handle paused requests

globalThis.fetch = async (input, init = {}) => {
  let url = typeof input === "string" ? input : (input instanceof Request ? input.url : input?.url || "");
  if (url && !url.includes('://')) {
    try {
      const base = _domParse("document_url") || "about:blank";
      url = new URL(url, base).href;
    } catch(e) { /* keep as-is if URL resolution fails */ }
  }
  const method = init.method || (input instanceof Request ? input.method : "GET");
  const hdrs = JSON.stringify(init.headers instanceof Headers ? Object.fromEntries(init.headers.entries()) : init.headers || {});
  const body = init.body ? String(init.body) : "";
  const fetchMode = init.mode || (input instanceof Request ? input.mode : "cors");
  const pageOrigin = (function() { try { const u = new URL(_domParse("document_url") || "about:blank"); return u.origin; } catch(e) { return ""; } })();
  const raw = await Deno.core.ops.op_fetch_url(url, method, hdrs, body, pageOrigin, fetchMode);
  const parsed = JSON.parse(raw);
  if (parsed.blocked) {
    const err = new TypeError('net::ERR_FAILED');
    err.name = 'AbortError';
    err.__aborted = true;
    throw err;
  }
  if (parsed.corsBlocked) {
    throw new TypeError('Failed to fetch: ' + (parsed.corsError || 'CORS error'));
  }
  const respType = parsed.status === 0 ? "opaque" : (fetchMode === "no-cors" ? "opaque" : "basic");
  return {
    ok: parsed.status >= 200 && parsed.status < 300, status: parsed.status, statusText: "",
    type: respType, redirected: false, url: parsed.url || url,
    headers: new Headers(parsed.headers || {}),
    async text() { return parsed.body || ""; },
    async json() { return JSON.parse(parsed.body || "null"); },
    async arrayBuffer() { return new TextEncoder().encode(parsed.body || "").buffer; },
    async blob() { return new Blob([parsed.body || ""]); },
    clone() { return this; },
  };
};

if (typeof Headers === "undefined") {
  globalThis.Headers = class Headers {
    constructor(init={}) { this._h={}; if(init) { if(init instanceof Headers) { init.forEach((v,k)=>{this._h[k]=v;}); } else if(typeof init==="object") { for(const[k,v]of Object.entries(init)) this._h[k.toLowerCase()]=String(v); } } }
    get(n) { return this._h[n.toLowerCase()]??null; } set(n,v) { this._h[n.toLowerCase()]=String(v); }
    has(n) { return n.toLowerCase() in this._h; } delete(n) { delete this._h[n.toLowerCase()]; }
    append(n,v) { this._h[n.toLowerCase()]=String(v); }
    forEach(cb) { for(const[k,v] of Object.entries(this._h)) cb(v,k,this); }
    entries() { return Object.entries(this._h)[Symbol.iterator](); }
    keys() { return Object.keys(this._h)[Symbol.iterator](); }
    values() { return Object.values(this._h)[Symbol.iterator](); }
    [Symbol.iterator]() { return this.entries(); }
  };
}

globalThis.XMLHttpRequest = class XMLHttpRequest {
  static UNSENT = 0;
  static OPENED = 1;
  static HEADERS_RECEIVED = 2;
  static LOADING = 3;
  static DONE = 4;
  UNSENT = 0; OPENED = 1; HEADERS_RECEIVED = 2; LOADING = 3; DONE = 4;

  constructor() {
    this.readyState = 0;
    this.status = 0;
    this.statusText = "";
    this.responseText = "";
    this.responseXML = null;
    this.responseURL = "";
    this.responseType = "";
    this.response = null;
    this.timeout = 0;
    this.withCredentials = false;
    this.upload = { addEventListener(){}, removeEventListener(){} };
    this._method = "GET";
    this._url = "";
    this._headers = {};
    this._responseHeaders = {};
    this._aborted = false;
    this._listeners = {};
    this.onreadystatechange = null;
    this.onload = null;
    this.onerror = null;
    this.onabort = null;
    this.onprogress = null;
    this.ontimeout = null;
    this.onloadstart = null;
    this.onloadend = null;
  }

  open(method, url, async_) {
    this._method = method;
    this._url = url;
    this._headers = {};
    this._responseHeaders = {};
    this._aborted = false;
    this.status = 0;
    this.statusText = "";
    this.responseText = "";
    this.response = null;
    this._setReadyState(1);
  }

  setRequestHeader(name, value) {
    this._headers[name] = value;
  }

  getResponseHeader(name) {
    const lower = name.toLowerCase();
    for (const [k, v] of Object.entries(this._responseHeaders)) {
      if (k.toLowerCase() === lower) return v;
    }
    return null;
  }

  getAllResponseHeaders() {
    return Object.entries(this._responseHeaders)
      .map(([k, v]) => k + ': ' + v)
      .join('\r\n');
  }

  overrideMimeType(mime) { this._overrideMime = mime; }

  send(body) {
    if (this.readyState !== 1) return;
    if (this._aborted) return;

    const xhr = this;
    this._fireEvent('loadstart');

    let url = this._url;
    if (url && !url.includes('://')) {
      try {
        const base = _domParse("document_url") || "about:blank";
        url = new URL(url, base).href;
      } catch(e) {}
    }

    fetch(url, {
      method: this._method,
      headers: this._headers,
      body: body || undefined,
      mode: 'cors',
    }).then(async (resp) => {
      if (xhr._aborted) return;

      xhr.status = resp.status;
      xhr.statusText = resp.statusText || '';
      xhr.responseURL = resp.url || url;

      if (resp.headers) {
        resp.headers.forEach((v, k) => { xhr._responseHeaders[k] = v; });
      }

      xhr._setReadyState(2); // HEADERS_RECEIVED

      const text = await resp.text();
      if (xhr._aborted) return;

      xhr.responseText = text;
      xhr._setReadyState(3); // LOADING

      switch (xhr.responseType) {
        case 'json':
          try { xhr.response = JSON.parse(text); } catch(e) { xhr.response = null; }
          break;
        case 'text':
        case '':
          xhr.response = text;
          break;
        case 'arraybuffer':
          xhr.response = new TextEncoder().encode(text).buffer;
          break;
        case 'blob':
          xhr.response = new Blob([text]);
          break;
        case 'document':
          xhr.response = text; // simplified
          break;
        default:
          xhr.response = text;
      }

      xhr._setReadyState(4); // DONE
      xhr._fireEvent('load');
      xhr._fireEvent('loadend');
    }).catch((err) => {
      if (xhr._aborted) return;
      xhr.status = 0;
      xhr.readyState = 4;
      xhr._fireEvent('readystatechange');
      if (err && err.__aborted) {
        xhr._aborted = true;
        xhr._fireEvent('abort');
        xhr._fireEvent('loadend');
        if (xhr.onabort) xhr.onabort(err);
      } else {
        xhr._fireEvent('error');
        xhr._fireEvent('loadend');
        if (xhr.onerror) xhr.onerror(err);
      }
    });
  }

  abort() {
    this._aborted = true;
    if (this.readyState > 0 && this.readyState < 4) {
      this._setReadyState(4);
      this._fireEvent('abort');
      this._fireEvent('loadend');
    }
    this.readyState = 0;
  }

  addEventListener(type, handler) {
    if (!this._listeners[type]) this._listeners[type] = [];
    this._listeners[type].push(handler);
  }

  removeEventListener(type, handler) {
    if (this._listeners[type]) {
      this._listeners[type] = this._listeners[type].filter(h => h !== handler);
    }
  }

  _setReadyState(state) {
    this.readyState = state;
    this._fireEvent('readystatechange');
    if (this.onreadystatechange) {
      try { this.onreadystatechange(); } catch(e) {}
    }
  }

  _fireEvent(type) {
    const event = { type, target: this, currentTarget: this, bubbles: false };
    const handlers = this._listeners[type] || [];
    for (const h of handlers) { try { h.call(this, event); } catch(e) {} }
    const prop = 'on' + type;
    if (type !== 'readystatechange' && typeof this[prop] === 'function') {
      try { this[prop](event); } catch(e) {}
    }
  }
};
_markNative(XMLHttpRequest);
_markNative(XMLHttpRequest.prototype.open);
_markNative(XMLHttpRequest.prototype.send);
_markNative(XMLHttpRequest.prototype.abort);
_markNative(XMLHttpRequest.prototype.setRequestHeader);
_markNative(XMLHttpRequest.prototype.getResponseHeader);
_markNative(XMLHttpRequest.prototype.getAllResponseHeaders);

if (typeof URL === 'undefined' || !URL.prototype) {
  globalThis.URL = class URL {
    constructor(url, base) {
      let full = url;
      if (base && !url.includes('://')) {
        var bm = base.match(/^(https?:\/\/[^\/\?#]+)(\/[^?#]*)?/);
        if (bm) {
          var bOrigin = bm[1];
          var bPath = bm[2] || '/';
          if (url.startsWith('/')) {
            full = bOrigin + url;
          } else if (url.startsWith('?') || url.startsWith('#')) {
            full = bOrigin + bPath + url;
          } else {
            var dir = bPath.substring(0, bPath.lastIndexOf('/') + 1);
            full = bOrigin + dir + url;
          }
        }
      }
      const m = full.match(/^(https?):\/\/([^\/\?#]+)(\/[^?#]*)?(\?[^#]*)?(#.*)?$/);
      if (m) {
        this.protocol = m[1] + ':';
        this.host = m[2]; this.hostname = m[2].split(':')[0];
        this.port = m[2].includes(':') ? m[2].split(':')[1] : '';
        this.pathname = m[3] || '/';
        this.search = m[4] || ''; this.hash = m[5] || '';
      } else {
        this.protocol = ''; this.host = ''; this.hostname = '';
        this.port = ''; this.pathname = full; this.search = ''; this.hash = '';
      }
      this.href = full; this.origin = this.protocol + '//' + this.host;
      this.searchParams = new URLSearchParams(this.search);
    }
    toString() { return this.href; }
    toJSON() { return this.href; }
    static createObjectURL() { return 'blob:null/fake-' + Math.random().toString(36).slice(2); }
    static revokeObjectURL() {}
  };
}

globalThis.requestIdleCallback = globalThis.requestIdleCallback || function requestIdleCallback(cb, opts) {
  const start = Date.now();
  return setTimeout(() => {
    cb({
      didTimeout: false,
      timeRemaining() { return Math.max(0, 50 - (Date.now() - start)); },
    });
  }, 1);
};
globalThis.cancelIdleCallback = globalThis.cancelIdleCallback || function cancelIdleCallback(id) { clearTimeout(id); };
_markNative(globalThis.requestIdleCallback);
_markNative(globalThis.cancelIdleCallback);

if (typeof Request === 'undefined') {
  globalThis.Request = class Request {
    constructor(input, init = {}) {
      if (typeof input === 'string') { this.url = input; }
      else if (input instanceof Request) { this.url = input.url; init = { ...input, ...init }; }
      else { this.url = String(input); }
      this.method = (init.method || 'GET').toUpperCase();
      this.headers = new Headers(init.headers);
      this.body = init.body || null;
      this.mode = init.mode || 'cors';
      this.credentials = init.credentials || 'same-origin';
      this.redirect = init.redirect || 'follow';
      this.referrer = init.referrer || '';
      this.signal = init.signal || { aborted: false, addEventListener(){}, removeEventListener(){} };
      this.cache = init.cache || 'default';
    }
    clone() { return new Request(this.url, { method: this.method, headers: this.headers, body: this.body }); }
    async text() { return this.body ? String(this.body) : ''; }
    async json() { return JSON.parse(await this.text()); }
    async arrayBuffer() { return new TextEncoder().encode(await this.text()).buffer; }
  };
}

if (typeof Response === 'undefined') {
  globalThis.Response = class Response {
    constructor(body, init = {}) {
      this._body = body; this.status = init.status || 200; this.statusText = init.statusText || '';
      this.ok = this.status >= 200 && this.status < 300;
      this.headers = new Headers(init.headers);
      this.type = 'basic'; this.url = '';
    }
    async text() { return this._body ? String(this._body) : ''; }
    async json() { return JSON.parse(await this.text()); }
    async arrayBuffer() { return new TextEncoder().encode(await this.text()).buffer; }
    async blob() { return new Blob([await this.text()]); }
    clone() { return new Response(this._body, { status: this.status, headers: this.headers }); }
    static error() { return new Response(null, { status: 0 }); }
    static redirect(url, status) { return new Response(null, { status: status || 302, headers: { Location: url } }); }
    static json(data, init) { return new Response(JSON.stringify(data), { ...init, headers: { 'content-type': 'application/json', ...(init?.headers || {}) } }); }
  };
}

if (!Element.prototype.replaceWith) {
  Element.prototype.replaceWith = function(...nodes) {
    const parent = this.parentNode;
    if (!parent) return;
    for (const n of nodes) {
      if (typeof n === 'string') parent.insertBefore(document.createTextNode(n), this);
      else parent.insertBefore(n, this);
    }
    parent.removeChild(this);
  };
  _markNative(Element.prototype.replaceWith);
}
if (!Element.prototype.before) {
  Element.prototype.before = function(...nodes) {
    const parent = this.parentNode;
    if (!parent) return;
    for (const n of nodes) {
      if (typeof n === 'string') parent.insertBefore(document.createTextNode(n), this);
      else parent.insertBefore(n, this);
    }
  };
  _markNative(Element.prototype.before);
}
if (!Element.prototype.after) {
  Element.prototype.after = function(...nodes) {
    const parent = this.parentNode;
    if (!parent) return;
    const ref = this.nextSibling;
    for (const n of nodes) {
      if (typeof n === 'string') parent.insertBefore(document.createTextNode(n), ref);
      else parent.insertBefore(n, ref);
    }
  };
  _markNative(Element.prototype.after);
}

if (!('isConnected' in Node.prototype)) {
  Object.defineProperty(Node.prototype, 'isConnected', {
    get() {
      let node = this;
      while (node) {
        if (node.nodeType === 9) return true; // Document node
        node = node.parentNode;
      }
      return false;
    }
  });
}

