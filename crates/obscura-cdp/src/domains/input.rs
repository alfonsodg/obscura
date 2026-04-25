use serde_json::{json, Value};

use crate::dispatch::CdpContext;

/// Escapes a string for safe interpolation into JavaScript single-quoted strings.
/// Backslashes must be escaped first to avoid double-escaping.
fn escape_js_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\0', "\\0")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

pub async fn handle(
    method: &str,
    params: &Value,
    ctx: &mut CdpContext,
    session_id: &Option<String>,
) -> Result<Value, String> {
    match method {
        "dispatchMouseEvent" => {
            let event_type = params.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let x = params.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let y = params.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let button = params.get("button").and_then(|v| v.as_str()).unwrap_or("left");
            let click_count = params.get("clickCount").and_then(|v| v.as_u64()).unwrap_or(1);

            if event_type == "mousePressed" {
                if let Some(page) = ctx.get_session_page_mut(session_id) {
                    let code = format!(
                        "(function() {{\
                            var target = globalThis.__obscura_click_target || document.activeElement || document.body;\
                            if (!target) return;\
                            var evt = new MouseEvent('mousedown', {{bubbles:true,cancelable:true,clientX:{x},clientY:{y},button:0}});\
                            target.dispatchEvent(evt);\
                            var click = new MouseEvent('click', {{bubbles:true,cancelable:true,clientX:{x},clientY:{y},button:0}});\
                            var cancelled = !target.dispatchEvent(click);\
                            if (!cancelled) {{\
                                var link = target.closest ? target.closest('a[href]') : null;\
                                if (!link && target.tagName === 'A' && target.getAttribute('href')) link = target;\
                                if (link) {{\
                                    var href = link.getAttribute('href');\
                                    if (href && !href.startsWith('#') && !href.startsWith('javascript:')) {{\
                                        location.assign(href);\
                                    }}\
                                }}\
                            }}\
                        }})()",
                        x = x, y = y,
                    );
                    page.evaluate(&code);
                }
            } else if event_type == "mouseReleased" {
                if let Some(page) = ctx.get_session_page_mut(session_id) {
                    let code = format!(
                        "(function() {{\
                            var target = globalThis.__obscura_click_target || document.activeElement || document.body;\
                            if (!target) return;\
                            var evt = new MouseEvent('mouseup', {{bubbles:true,cancelable:true,clientX:{x},clientY:{y},button:0}});\
                            target.dispatchEvent(evt);\
                        }})()",
                        x = x, y = y,
                    );
                    page.evaluate(&code);
                }
            }

            Ok(json!({}))
        }
        "dispatchKeyEvent" => {
            let event_type = params.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let key = params.get("key").and_then(|v| v.as_str()).unwrap_or("");
            let code = params.get("code").and_then(|v| v.as_str()).unwrap_or("");
            let text = params.get("text").and_then(|v| v.as_str()).unwrap_or("");

            if let Some(page) = ctx.get_session_page_mut(session_id) {
                match event_type {
                    "keyDown" | "rawKeyDown" => {
                        let js = format!(
                            "(function() {{\
                                var target = document.activeElement || document.body;\
                                var evt = new KeyboardEvent('keydown', {{bubbles:true,cancelable:true,key:'{key}',code:'{code}'}});\
                                target.dispatchEvent(evt);\
                            }})()",
                            key = escape_js_string(key),
                            code = escape_js_string(code),
                        );
                        page.evaluate(&js);

                        if !text.is_empty() && text != "\r" && text != "\n" {
                            let js = format!(
                                "(function() {{\
                                    var target = document.activeElement;\
                                    if (target && (target.localName === 'input' || target.localName === 'textarea')) {{\
                                        target.value = (target.value || '') + '{text}';\
                                        target.dispatchEvent(new Event('input', {{bubbles:true}}));\
                                    }}\
                                }})()",
                                text = escape_js_string(text),
                            );
                            page.evaluate(&js);
                        }

                        if key == "Enter" {
                            let js = "(function() {\
                                var target = document.activeElement;\
                                if (target) {\
                                    target.dispatchEvent(new KeyboardEvent('keypress', {bubbles:true,key:'Enter',code:'Enter'}));\
                                    var form = target.form || target.closest && target.closest('form');\
                                    if (form && typeof form.submit === 'function') form.submit();\
                                }\
                            })()";
                            page.evaluate(js);
                        }

                        if key == "Backspace" {
                            let js = "(function() {\
                                var target = document.activeElement;\
                                if (target && (target.localName === 'input' || target.localName === 'textarea')) {\
                                    target.value = target.value.slice(0, -1);\
                                    target.dispatchEvent(new Event('input', {bubbles:true}));\
                                }\
                            })()";
                            page.evaluate(js);
                        }
                    }
                    "keyUp" => {
                        let js = format!(
                            "(function() {{\
                                var target = document.activeElement || document.body;\
                                var evt = new KeyboardEvent('keyup', {{bubbles:true,key:'{key}',code:'{code}'}});\
                                target.dispatchEvent(evt);\
                            }})()",
                            key = escape_js_string(key),
                            code = escape_js_string(code),
                        );
                        page.evaluate(&js);
                    }
                    "char" => {
                        if !text.is_empty() {
                            let js = format!(
                                "(function() {{\
                                    var target = document.activeElement;\
                                    if (target && (target.localName === 'input' || target.localName === 'textarea')) {{\
                                        target.value = (target.value || '') + '{text}';\
                                        target.dispatchEvent(new Event('input', {{bubbles:true}}));\
                                    }}\
                                }})()",
                                text = escape_js_string(text),
                            );
                            page.evaluate(&js);
                        }
                    }
                    _ => {}
                }
            }

            Ok(json!({}))
        }
        "dispatchTouchEvent" => Ok(json!({})),
        "setIgnoreInputEvents" => Ok(json!({})),
        _ => Err(format!("Unknown Input method: {}", method)),
    }
}

#[cfg(test)]
mod tests {
    use super::escape_js_string;

    #[test]
    fn escapes_backslash() {
        assert_eq!(escape_js_string("a\\b"), "a\\\\b");
    }

    #[test]
    fn escapes_single_quote() {
        assert_eq!(escape_js_string("it's"), "it\\'s");
    }

    #[test]
    fn escapes_newline_and_cr() {
        assert_eq!(escape_js_string("a\nb\rc"), "a\\nb\\rc");
    }

    #[test]
    fn escapes_null_byte() {
        assert_eq!(escape_js_string("a\0b"), "a\\0b");
    }

    #[test]
    fn escapes_combined() {
        assert_eq!(escape_js_string("\\'\n\r\0"), "\\\\\\'\\n\\r\\0");
    }
}
