use serde_json::json;
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use crate::dispatch::{self, CdpContext};
use crate::types::CdpRequest;

pub(crate) async fn process_cdp_message(
    text: &str,
    ctx: &mut CdpContext,
    reply_tx: &mpsc::UnboundedSender<String>,
) {
    let req: CdpRequest = match serde_json::from_str(text) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("Invalid CDP: {}: {}", e, &text[..text.len().min(200)]);
            return;
        }
    };

    tracing::debug!("CDP: {} (id={}, s={:?})", req.method, req.id, req.session_id);

    let response = dispatch::dispatch(&req, ctx).await;

    if let Ok(json) = serde_json::to_string(&response) {
        let _ = reply_tx.send(json);
    }

    for event in ctx.pending_events.drain(..) {
        if let Ok(json) = serde_json::to_string(&event) {
            let _ = reply_tx.send(json);
        }
    }

    if let Some((nav_url, nav_method, nav_body)) = check_pending_navigation(ctx, &req.session_id) {
        tracing::info!("JS-triggered nav: {} {} (body: {} bytes)", nav_method, nav_url, nav_body.len());
        let nav_req = CdpRequest {
            id: 0,
            method: "Page.navigate".to_string(),
            params: json!({"url": nav_url, "__method": nav_method, "__body": nav_body}),
            session_id: req.session_id.clone(),
        };
        let _ = dispatch::dispatch(&nav_req, ctx).await;
        for event in ctx.pending_events.drain(..) {
            if let Ok(json) = serde_json::to_string(&event) {
                let _ = reply_tx.send(json);
            }
        }
    }
}

pub(crate) fn fast_path_response(text: &str) -> Option<String> {
    let req: CdpRequest = serde_json::from_str(text).ok()?;

    let result = match req.method.as_str() {
        "Network.enable" | "Network.setCacheDisabled" | "Network.setRequestInterception" |
        "Page.enable" | "Page.setLifecycleEventsEnabled" | "Page.setInterceptFileChooserDialog" |
        "Runtime.runIfWaitingForDebugger" | "Runtime.discardConsoleEntries" |
        "Performance.enable" | "Log.enable" | "Security.enable" |
        "Emulation.setDeviceMetricsOverride" | "Emulation.setTouchEmulationEnabled" |
        "CSS.enable" | "Accessibility.enable" | "ServiceWorker.enable" |
        "Inspector.enable" | "Debugger.enable" | "Profiler.enable" |
        "HeapProfiler.enable" | "Overlay.enable" | "Storage.enable" |
        "Target.setAutoAttach" => {
            Some(json!({}))
        }
        "Browser.getVersion" => {
            Some(json!({
                "protocolVersion": "1.3",
                "product": "Obscura/0.1.0",
                "revision": "0",
                "userAgent": "Obscura/0.1.0",
                "jsVersion": "V8",
            }))
        }
        "Browser.setDownloadBehavior" | "Browser.getWindowBounds" => {
            Some(json!({}))
        }
        _ => None,
    };

    if let Some(value) = result {
        let resp = crate::types::CdpResponse::success(req.id, value, req.session_id);
        serde_json::to_string(&resp).ok()
    } else {
        None
    }
}

fn check_pending_navigation(ctx: &CdpContext, session_id: &Option<String>) -> Option<(String, String, String)> {
    let page_id = session_id
        .as_ref()
        .and_then(|sid| ctx.sessions.get(sid))?;
    let page = ctx.pages.iter().find(|p| &p.id == page_id)?;
    page.take_pending_navigation()
}

pub(crate) async fn handle_http_json(stream: TcpStream, port: u16, endpoint: &str) -> anyhow::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut stream = stream;
    let mut buf = vec![0u8; 4096];
    let _ = stream.read(&mut buf).await?;

    let body = match endpoint {
        "version" => serde_json::to_string_pretty(&json!({
            "Browser": "Obscura/0.1.0",
            "Protocol-Version": "1.3",
            "User-Agent": "Obscura/0.1.0 (Headless Browser)",
            "V8-Version": "N/A",
            "WebKit-Version": "N/A",
            "webSocketDebuggerUrl": format!("ws://127.0.0.1:{}/devtools/browser", port),
        }))?,
        "list" => serde_json::to_string_pretty(&json!([{
            "description": "",
            "devtoolsFrontendUrl": "",
            "id": "page-1",
            "title": "",
            "type": "page",
            "url": "about:blank",
            "webSocketDebuggerUrl": format!("ws://127.0.0.1:{}/devtools/page/page-1", port),
        }]))?,
        "protocol" => {
            serde_json::to_string_pretty(&json!({ "version": { "major": "1", "minor": "3" } }))?
        }
        _ => "{}".to_string(),
    };

    let resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(), body,
    );
    stream.write_all(resp.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

pub(crate) async fn handle_health(stream: TcpStream) -> anyhow::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut stream = stream;
    let mut buf = vec![0u8; 4096];
    let _ = stream.read(&mut buf).await?;

    let body = json!({"status": "ok", "service": "obscura", "version": "0.1.0"}).to_string();
    let resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(), body,
    );
    stream.write_all(resp.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}
