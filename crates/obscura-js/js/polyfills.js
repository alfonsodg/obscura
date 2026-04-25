if (typeof ReadableStream === 'undefined') {
  globalThis.ReadableStream = class ReadableStream {
    constructor(source = {}, strategy = {}) {
      this._source = source; this._queue = []; this._closed = false;
      this.locked = false;
      if (source.start) source.start({ enqueue: (chunk) => this._queue.push(chunk), close: () => { this._closed = true; }, error: () => {} });
    }
    getReader() {
      this.locked = true;
      const stream = this;
      return {
        read() {
          if (stream._queue.length > 0) return Promise.resolve({ value: stream._queue.shift(), done: false });
          if (stream._closed) return Promise.resolve({ value: undefined, done: true });
          return Promise.resolve({ value: undefined, done: true });
        },
        releaseLock() { stream.locked = false; },
        cancel() { stream._closed = true; return Promise.resolve(); },
        get closed() { return stream._closed ? Promise.resolve() : new Promise(() => {}); },
      };
    }
    cancel() { this._closed = true; return Promise.resolve(); }
    pipeTo(dest) { return Promise.resolve(); }
    pipeThrough(transform) { return transform.readable || new ReadableStream(); }
    tee() { return [new ReadableStream(), new ReadableStream()]; }
    [Symbol.asyncIterator]() {
      const reader = this.getReader();
      return { next: () => reader.read(), return: () => { reader.releaseLock(); return Promise.resolve({done:true}); } };
    }
  };
}
if (typeof WritableStream === 'undefined') {
  globalThis.WritableStream = class WritableStream {
    constructor(sink = {}) { this._sink = sink; this.locked = false; }
    getWriter() {
      this.locked = true;
      const stream = this;
      return {
        write(chunk) { if (stream._sink.write) stream._sink.write(chunk); return Promise.resolve(); },
        close() { if (stream._sink.close) stream._sink.close(); return Promise.resolve(); },
        abort() { return Promise.resolve(); },
        releaseLock() { stream.locked = false; },
        get ready() { return Promise.resolve(); },
        get closed() { return Promise.resolve(); },
        get desiredSize() { return 1; },
      };
    }
    close() { return Promise.resolve(); }
    abort() { return Promise.resolve(); }
  };
}
if (typeof TransformStream === 'undefined') {
  globalThis.TransformStream = class TransformStream {
    constructor(transformer = {}) {
      this.readable = new ReadableStream();
      this.writable = new WritableStream();
    }
  };
}

if (!globalThis.crypto) globalThis.crypto = {};
if (!globalThis.crypto.subtle) {
  const _notSupported = (name) => { throw new DOMException(`crypto.subtle.${name}() is not implemented in Obscura`, 'NotSupportedError'); };
  globalThis.crypto.subtle = {
    async digest() { _notSupported('digest'); },
    async encrypt() { _notSupported('encrypt'); },
    async decrypt() { _notSupported('decrypt'); },
    async sign() { _notSupported('sign'); },
    async verify() { _notSupported('verify'); },
    async generateKey() { _notSupported('generateKey'); },
    async importKey() { _notSupported('importKey'); },
    async exportKey() { _notSupported('exportKey'); },
    async deriveBits() { _notSupported('deriveBits'); },
    async deriveKey() { _notSupported('deriveKey'); },
    async wrapKey() { _notSupported('wrapKey'); },
    async unwrapKey() { _notSupported('unwrapKey'); },
  };
}

if (typeof DOMRect === 'undefined') {
  globalThis.DOMRect = class DOMRect {
    constructor(x=0,y=0,w=0,h=0) { this.x=x;this.y=y;this.width=w;this.height=h;this.top=y;this.right=x+w;this.bottom=y+h;this.left=x; }
    toJSON() { return {x:this.x,y:this.y,width:this.width,height:this.height,top:this.top,right:this.right,bottom:this.bottom,left:this.left}; }
    static fromRect(r={}) { return new DOMRect(r.x,r.y,r.width,r.height); }
  };
}
if (typeof DOMPoint === 'undefined') {
  globalThis.DOMPoint = class DOMPoint {
    constructor(x=0,y=0,z=0,w=1) { this.x=x;this.y=y;this.z=z;this.w=w; }
    static fromPoint(p={}) { return new DOMPoint(p.x,p.y,p.z,p.w); }
  };
}
if (typeof DOMMatrix === 'undefined') {
  globalThis.DOMMatrix = class DOMMatrix {
    constructor() { this.a=1;this.b=0;this.c=0;this.d=1;this.e=0;this.f=0;this.is2D=true;this.isIdentity=true; }
    static fromMatrix() { return new DOMMatrix(); }
    static fromFloat32Array() { return new DOMMatrix(); }
    static fromFloat64Array() { return new DOMMatrix(); }
    multiply() { return new DOMMatrix(); }
    inverse() { return new DOMMatrix(); }
    translate() { return new DOMMatrix(); }
    scale() { return new DOMMatrix(); }
    rotate() { return new DOMMatrix(); }
    transformPoint(p) { return new DOMPoint(p?.x||0,p?.y||0); }
  };
}

if (typeof Image === 'undefined') {
  globalThis.Image = class Image {
    constructor(w, h) { this.width = w || 0; this.height = h || 0; this.src = ''; this.onload = null; this.onerror = null; this.complete = false; this.naturalWidth = 0; this.naturalHeight = 0; }
    addEventListener() {} removeEventListener() {}
    setAttribute(k, v) { this[k] = v; if (k === 'src' && this.onload) setTimeout(() => { this.complete = true; this.onload(); }, 0); }
    getAttribute(k) { return this[k]; }
  };
}

if (typeof Audio === 'undefined') {
  globalThis.Audio = class Audio {
    constructor(src) { this.src = src || ''; this.paused = true; this.volume = 1; this.currentTime = 0; this.duration = 0; }
    play() { return Promise.resolve(); } pause() { this.paused = true; } load() {}
    addEventListener() {} removeEventListener() {}
  };
}

if (typeof FileReader === 'undefined') {
  globalThis.FileReader = class FileReader {
    constructor() { this.result = null; this.readyState = 0; this.onload = null; this.onerror = null; }
    readAsText(blob) { if (blob?.text) blob.text().then(t => { this.result = t; this.readyState = 2; if (this.onload) this.onload({target:this}); }); }
    readAsDataURL(blob) { this.result = 'data:;base64,'; this.readyState = 2; if (this.onload) setTimeout(() => this.onload({target:this}), 0); }
    readAsArrayBuffer(blob) { this.result = new ArrayBuffer(0); this.readyState = 2; if (this.onload) setTimeout(() => this.onload({target:this}), 0); }
    abort() { this.readyState = 0; }
    addEventListener(t, fn) { if (t === 'load') this.onload = fn; }
    removeEventListener() {}
  };
}

if (typeof EventSource === 'undefined') {
  globalThis.EventSource = class EventSource {
    constructor(url) { this.url = url; this.readyState = 0; this.onopen = null; this.onmessage = null; this.onerror = null; }
    close() { this.readyState = 2; }
    addEventListener() {} removeEventListener() {}
    static CONNECTING = 0; static OPEN = 1; static CLOSED = 2;
  };
}

if (typeof WebSocket === 'undefined') {
  globalThis.WebSocket = class WebSocket {
    constructor(url, protocols) { this.url = url; this.readyState = 0; this.bufferedAmount = 0; this.onopen = null; this.onmessage = null; this.onerror = null; this.onclose = null; this.protocol = ''; }
    send(data) {} close(code, reason) { this.readyState = 3; if (this.onclose) this.onclose({code:code||1000,reason:reason||'',wasClean:true}); }
    addEventListener() {} removeEventListener() {}
    static CONNECTING = 0; static OPEN = 1; static CLOSING = 2; static CLOSED = 3;
  };
}

if (typeof BroadcastChannel === 'undefined') {
  globalThis.BroadcastChannel = class BroadcastChannel {
    constructor(name) { this.name = name; this.onmessage = null; }
    postMessage(msg) {} close() {}
    addEventListener() {} removeEventListener() {}
  };
}

if (typeof MediaQueryList === 'undefined') {
  globalThis.MediaQueryList = class MediaQueryList {
    constructor(q) { this.media = q || ''; this.matches = false; }
    addListener() {} removeListener() {} addEventListener() {} removeEventListener() {}
  };
}

if (typeof ImageData === 'undefined') {
  globalThis.ImageData = class ImageData {
    constructor(w, h) {
      if (w instanceof Uint8ClampedArray) { this.data = w; this.width = h; this.height = w.length / (4 * h); }
      else { this.width = w; this.height = h; this.data = new Uint8ClampedArray(w * h * 4); }
    }
  };
}

if (typeof CanvasRenderingContext2D === 'undefined') {
  globalThis.CanvasRenderingContext2D = class CanvasRenderingContext2D {};
}

if (typeof OffscreenCanvas === 'undefined') {
  globalThis.OffscreenCanvas = class OffscreenCanvas {
    constructor(w, h) { this.width = w; this.height = h; }
    getContext(type) { return globalThis.document?.createElement('canvas')?.getContext(type) || null; }
    convertToBlob() { return Promise.resolve(new Blob([''])); }
    transferToImageBitmap() { return {}; }
  };
}

if (typeof Path2D === 'undefined') {
  globalThis.Path2D = class Path2D { constructor(){} moveTo(){} lineTo(){} arc(){} rect(){} closePath(){} addPath(){} };
}

if (typeof ImageBitmap === 'undefined') {
  globalThis.ImageBitmap = class ImageBitmap { constructor(){this.width=0;this.height=0;} close(){} };
  globalThis.createImageBitmap = function() { return Promise.resolve(new ImageBitmap()); };
}

if (typeof Selection === 'undefined') {
  globalThis.Selection = class Selection {
    constructor(){this.anchorNode=null;this.focusNode=null;this.rangeCount=0;this.isCollapsed=true;this.type='None';}
    getRangeAt(){return null;} collapse(){} extend(){} selectAllChildren(){} deleteFromDocument(){}
    addRange(){} removeRange(){} removeAllRanges(){} toString(){return '';}
  };
}

if (typeof NodeFilter === 'undefined') {
  globalThis.NodeFilter = { SHOW_ALL:0xFFFFFFFF, SHOW_ELEMENT:1, SHOW_TEXT:4, SHOW_COMMENT:128,
    FILTER_ACCEPT:1, FILTER_REJECT:2, FILTER_SKIP:3 };
}

if (typeof TreeWalker === 'undefined') {
  globalThis.TreeWalker = class TreeWalker {
    constructor(root){this.root=root;this.currentNode=root;this.whatToShow=0xFFFFFFFF;this.filter=null;}
    parentNode(){return this.currentNode?.parentNode||null;}
    firstChild(){return this.currentNode?.firstChild||null;}
    lastChild(){return this.currentNode?.lastChild||null;}
    previousSibling(){return this.currentNode?.previousSibling||null;}
    nextSibling(){return this.currentNode?.nextSibling||null;}
    nextNode(){return null;} previousNode(){return null;}
  };
}

if (typeof Range === 'undefined') {
  globalThis.Range = class Range {
    constructor(){this.startContainer=null;this.startOffset=0;this.endContainer=null;this.endOffset=0;this.collapsed=true;this.commonAncestorContainer=null;}
    setStart(n,o){this.startContainer=n;this.startOffset=o;} setEnd(n,o){this.endContainer=n;this.endOffset=o;}
    collapse(){} selectNode(){} selectNodeContents(){} cloneContents(){return document?.createDocumentFragment();}
    deleteContents(){} insertNode(){} getBoundingClientRect(){return new DOMRect();}
    getClientRects(){return [];} cloneRange(){return new Range();} toString(){return '';}
  };
}

if (typeof SharedWorker === 'undefined') {
  globalThis.SharedWorker = class SharedWorker {
    constructor() { this.port = { postMessage(){}, onmessage:null, start(){}, close(){}, addEventListener(){}, removeEventListener(){} }; this.onerror = null; }
  };
}
if (typeof ServiceWorkerContainer === 'undefined') {
  globalThis.ServiceWorkerContainer = class { register(){return Promise.resolve();} getRegistrations(){return Promise.resolve([]);} };
}

if (typeof URLPattern === 'undefined') {
  globalThis.URLPattern = class URLPattern {
    constructor(pattern){this._pattern=pattern||{};} test(){return false;} exec(){return null;}
  };
}

if (typeof Document !== 'undefined' && !Document.prototype.importNode) {
  Document.prototype.importNode = function(node, deep) { return node?.cloneNode(!!deep) || null; };
}

