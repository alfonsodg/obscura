globalThis.__obscura_init = function() {
  _fpSeed = Date.now() ^ (Math.random() * 0xFFFFFFFF >>> 0);
  _fpCache = null;

  globalThis.document = new Document(+_dom("document_node_id"));

  const scr = _fp('screen');
  const sw = scr[0], sh = scr[1];
  globalThis.screen = { width:sw, height:sh, availWidth:sw, availHeight:sh-40, colorDepth:24, pixelDepth:24, availTop:0, availLeft:0, orientation:{type:"landscape-primary",angle:0,addEventListener(){},removeEventListener(){},dispatchEvent(){return true;}} };
  globalThis.visualViewport = { width:sw, height:sh-80, offsetLeft:0, offsetTop:0, scale:1, addEventListener(){}, removeEventListener(){} };
  globalThis.devicePixelRatio = sw >= 2560 ? 2 : 1;
  globalThis.innerWidth = sw; globalThis.innerHeight = sh - 80;
  globalThis.outerWidth = sw; globalThis.outerHeight = sh;

  const t0 = Date.now();
  globalThis.performance.timeOrigin = t0;
  globalThis.performance.timing = { navigationStart: t0, domContentLoadedEventEnd: t0, loadEventEnd: t0 };

  const hide = (obj, props) => {
    for (const p of props) {
      if (p in obj) {
        try { Object.defineProperty(obj, p, { enumerable: false, configurable: true }); } catch(e) {}
      }
    }
  };
  const toHide = Object.keys(globalThis).filter(k =>
    k.startsWith('_') || k.includes('obscura') || k.includes('Obscura')
  );
  for (const p of toHide) {
    try { Object.defineProperty(globalThis, p, { enumerable: false }); } catch(e) {
    }
  }
  delete globalThis.__obscura_init;
};
