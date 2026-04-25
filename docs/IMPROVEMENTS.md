# Obscura — Changelog de mejoras (abril 2026)

Fork: `alfonsodg/obscura` | Branch: `fix/resolve-all-issues` | 10 commits

## Seguridad (3 fixes críticos)

- **SSRF protection**: `validate_url()` extraído a módulo compartido, aplicado en HTTP client, stealth client, y module loader. Bloquea localhost, IPs privadas, 169.254.169.254, file://, ftp://
- **JS injection en CDP**: `escape_js_string()` con orden correcto (backslash primero), escaping de \n, \r, \0, unicode separators. Aplicado en input.rs y dom.rs
- **crypto.subtle**: Todas las funciones lanzan `NotSupportedError` en vez de devolver resultados falsos (verify siempre true, digest FNV hash)

## DOM (6 mejoras — fix crítico de querySelector)

- **querySelector/querySelectorAll scoped**: `Element.querySelector()` ahora busca solo en el subtree del elemento, no en todo el documento. Agregados `query_selector_within()` y `query_selector_all_within()` al DomTree en Rust
- **DocumentFragment**: Implementación completa con tracking de hijos, appendChild, removeChild, insertBefore, cloneNode, querySelector
- **Element.attributes**: NamedNodeMap con Proxy, getNamedItem(), acceso por nombre
- **Node.parentNode setter**: Permite override para operaciones con fragments
- **Event handler properties**: onclick, onsubmit, onchange, onfocus, onblur, onload, onerror, oninput, onkeydown, onkeyup, onmousedown, onmouseup
- **setAttribute para on\***: Compila atributos on* como Function (comportamiento de browser real)

## CSS

- **getComputedStyle**: Reescrito con 40+ propiedades CSS por defecto, getPropertyValue directo, conversión camelCase/kebab-case bidireccional
- **CSSStyleDeclaration proxy**: Fix de cssText setter routing, almacenamiento bidireccional camelCase/kebab

## jQuery/Framework compatibility

- jQuery 1.8.3 (PrimeFaces/SEACE) ✅
- jQuery 1.9.1 (Books to Scrape) ✅
- jQuery 4.0.0 (jquery.com) ✅
- Bootstrap 3 ✅
- PrimeFaces (SEACE) — de cientos de errores a 1

## JavaScript Runtime

- **op_sleep**: Deno op async con `tokio::time::sleep` para delays reales
- **setTimeout**: Respeta delay parameter via op_sleep
- **setInterval**: Repite correctamente con delays reales
- **Dead code removed**: InterceptCallback, BOOTSTRAP_JS constant

## CDP Server

- **Runtime.enable en fast path**: Evita race condition con Puppeteer page.goto()
- **Dominios agregados**: Audits, Console, Database, DOMStorage, IndexedDB, WebMCP, Target.setDiscoverTargets
- **/health endpoint**: `{"status":"ok","service":"obscura","version":"0.1.0"}`
- **base64 crate**: Reemplaza decoder hand-rolled
- **Multi-message fast path**: Soporte para respuestas con eventos

## CLI

- **`obscura inspect <URL>`**: Diagnóstico completo de página — navegación, DOM structure, semantic HTML, meta tags, accesibilidad, errores JS, resumen con conteo de issues

## Arquitectura

- **bootstrap.js split**: 2925 líneas → 10 módulos (core, stealth, timers, dom, navigator, fetch, webapis, workers, polyfills, init)
- **Rust files split**: main.rs→4, server.rs→3, page.rs→2
- **102 unit tests**: SSRF validation (5), JS escaping (5), base64 (2), DOM (28), JS runtime (46), cookies/robots/blocklist (16)
- **12 E2E tests**: CDP + Puppeteer
- **CI workflow**: GitHub Actions (test, clippy, fmt)

## Calidad

- clippy auto-fixes + suppressions
- rustfmt aplicado a todo el workspace
- .gitignore para Rust
- Dependencias pinned (wreq =6.0.0-rc.28, deno_core =0.350)
- License fix: MIT → Apache-2.0
- DEFAULT_USER_AGENT constante compartida

## Sitios probados exitosamente

| Sitio | Datos extraídos |
|-------|----------------|
| Hacker News | 30 stories con título, URL, score, autor |
| Amazon | Título, rating, reviews |
| Wikipedia | Headings, párrafos, links, referencias |
| Reddit (old) | 25 posts con score y comentarios |
| BBC News | Headlines, links |
| Books to Scrape | 20 libros con título, precio, rating |
| Craigslist | Listings con título y precio |
| SEACE | Formularios PrimeFaces, jQuery funcional |
| GitHub API | Repos con stars, language |
| NPM Registry | Package info |
| quotes.toscrape | Quotes con autor y tags |
| httpbin | Headers, user-agent, IP |
