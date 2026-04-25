use std::collections::HashMap;

use serde_json::json;
use tokio::sync::mpsc;

use crate::dispatch::CdpContext;
use crate::server::ServerMessage;
use crate::types::CdpRequest;

pub(crate) fn handle_fetch_resolution(
    text: &str,
    _ctx: &mut CdpContext,
    reply_tx: &mpsc::UnboundedSender<String>,
    intercepted_paused: &mut HashMap<String, tokio::sync::oneshot::Sender<obscura_js::ops::InterceptResolution>>,
) {
    if let Ok(req) = serde_json::from_str::<CdpRequest>(text) {
        let method = req.method.as_str();
        let request_id = req.params.get("requestId").and_then(|v| v.as_str()).unwrap_or("");
        tracing::info!(
            "INTERCEPTION resolution: {} for {}, paused_count={}",
            method,
            request_id,
            intercepted_paused.len()
        );

        if let Some(resolver) = intercepted_paused.remove(request_id) {
            tracing::info!("INTERCEPTION resolved: {}", request_id);
            let resolution = match method {
                "Fetch.continueRequest" => obscura_js::ops::InterceptResolution::Continue {
                    url: None,
                    method: None,
                    headers: None,
                    body: None,
                },
                "Fetch.fulfillRequest" => {
                    let status = req.params.get("responseCode").and_then(|v| v.as_u64()).unwrap_or(200) as u16;
                    let raw_body = req.params.get("body").and_then(|v| v.as_str()).unwrap_or("");
                    let body = decode_base64(raw_body);
                    let headers = req
                        .params
                        .get("responseHeaders")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|h| {
                                    Some((
                                        h.get("name")?.as_str()?.to_string(),
                                        h.get("value")?.as_str()?.to_string(),
                                    ))
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    obscura_js::ops::InterceptResolution::Fulfill { status, headers, body }
                }
                "Fetch.failRequest" => {
                    let reason = req
                        .params
                        .get("errorReason")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Failed")
                        .to_string();
                    obscura_js::ops::InterceptResolution::Fail { reason }
                }
                _ => return,
            };
            let _ = resolver.send(resolution);
            let resp = crate::types::CdpResponse::success(req.id, json!({}), req.session_id);
            if let Ok(json) = serde_json::to_string(&resp) {
                let _ = reply_tx.send(json);
            }
        }
    }
}

pub(crate) async fn process_with_interception(
    text: &str,
    ctx: &mut CdpContext,
    reply_tx: &mpsc::UnboundedSender<String>,
    rx: &mut mpsc::UnboundedReceiver<ServerMessage>,
    intercept_rx: &mut Option<mpsc::UnboundedReceiver<obscura_js::ops::InterceptedRequest>>,
    intercepted_paused: &mut HashMap<String, tokio::sync::oneshot::Sender<obscura_js::ops::InterceptResolution>>,
) {
    let req: CdpRequest = match serde_json::from_str(text) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("Invalid CDP: {}", e);
            return;
        }
    };

    tracing::info!("INTERCEPTION navigate: {} (id={})", req.method, req.id);

    let session_id = &req.session_id;
    let page_id = session_id.as_ref().and_then(|sid| ctx.sessions.get(sid)).cloned();

    let page_id = match page_id {
        Some(id) => id,
        None => {
            crate::http::process_cdp_message(text, ctx, reply_tx).await;
            return;
        }
    };

    let page_index = ctx.pages.iter().position(|p| p.id == page_id);
    let mut page = match page_index {
        Some(idx) => ctx.pages.remove(idx),
        None => {
            crate::http::process_cdp_message(text, ctx, reply_tx).await;
            return;
        }
    };

    let url = req.params.get("url").and_then(|v| v.as_str()).unwrap_or("");
    let wait_until = req
        .params
        .get("waitUntil")
        .and_then(|v| {
            if let Some(s) = v.as_str() {
                Some(obscura_browser::WaitUntil::parse(s))
            } else if let Some(arr) = v.as_array() {
                arr.iter()
                    .filter_map(|item| item.as_str())
                    .map(obscura_browser::WaitUntil::parse)
                    .max_by_key(|w| match w {
                        obscura_browser::WaitUntil::DomContentLoaded => 0,
                        obscura_browser::WaitUntil::Load => 1,
                        obscura_browser::WaitUntil::NetworkIdle2 => 2,
                        obscura_browser::WaitUntil::NetworkIdle0 => 3,
                    })
            } else {
                None
            }
        })
        .unwrap_or(obscura_browser::WaitUntil::Load);

    let preload_scripts: Vec<String> = ctx.preload_scripts.iter().map(|(_, s)| s.clone()).collect();

    if let Some(tx) = &ctx.intercept_tx {
        page.set_intercept_tx(tx.clone());
    }

    let session_for_events = req.session_id.clone();
    let frame_id = page.frame_id.clone();
    let loader_id = format!("loader-{}", uuid::Uuid::new_v4());

    let (nav_done_tx, mut nav_done_rx) = mpsc::channel::<(obscura_browser::Page, Result<(), String>)>(1);
    let url_owned = url.to_string();

    tokio::task::spawn_local(async move {
        let result = page
            .navigate_with_wait(&url_owned, wait_until)
            .await
            .map_err(|e| e.to_string());
        for source in &preload_scripts {
            if let Err(e) = page.execute_preload_script(source) {
                tracing::debug!("Preload script error: {}", e);
            }
        }
        let _ = nav_done_tx.send((page, result)).await;
    });

    #[allow(clippy::needless_late_init)]
    let mut navigate_result: Result<(), String> = Ok(());
    #[allow(clippy::needless_late_init)]
    let mut page_back: Option<obscura_browser::Page> = None;

    loop {
        let has_irx = intercept_rx.is_some();

        tokio::select! {
            Some((returned_page, result)) = nav_done_rx.recv() => {
                page_back = Some(returned_page);
                navigate_result = result;
                break;
            }
            Some(intercepted) = async {
                if let Some(ref mut irx) = intercept_rx {
                    irx.recv().await
                } else {
                    std::future::pending().await
                }
            }, if has_irx => {
                tracing::info!("INTERCEPTION: requestPaused for {} {} (sending to client)", intercepted.method, intercepted.url);
                let rws_event = json!({
                    "method": "Network.requestWillBeSent",
                    "params": {
                        "requestId": intercepted.request_id,
                        "loaderId": "",
                        "documentURL": "",
                        "request": {
                            "url": intercepted.url,
                            "method": intercepted.method,
                            "headers": intercepted.headers,
                            "initialPriority": "High",
                            "referrerPolicy": "strict-origin-when-cross-origin",
                        },
                        "timestamp": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs_f64(),
                        "wallTime": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs_f64(),
                        "initiator": {"type": "script"},
                        "type": intercepted.resource_type,
                        "frameId": frame_id,
                    },
                    "sessionId": session_for_events,
                });
                let _ = reply_tx.send(rws_event.to_string());

                let event_json = json!({
                    "method": "Fetch.requestPaused",
                    "params": {
                        "requestId": intercepted.request_id,
                        "request": {
                            "url": intercepted.url,
                            "method": intercepted.method,
                            "headers": intercepted.headers,
                            "initialPriority": "High",
                            "referrerPolicy": "strict-origin-when-cross-origin",
                        },
                        "frameId": frame_id,
                        "resourceType": intercepted.resource_type,
                        "networkId": intercepted.request_id,
                        "responseErrorReason": null,
                        "responseStatusCode": null,
                        "responseHeaders": null,
                    },
                    "sessionId": session_for_events,
                });
                let event_str = event_json.to_string();
                tracing::info!("INTERCEPTION event JSON: {}", &event_str[..event_str.len().min(300)]);
                let _ = reply_tx.send(event_str);
                intercepted_paused.insert(intercepted.request_id.clone(), intercepted.resolver);
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            }
            Some(msg) = rx.recv() => {
                tracing::info!("INTERCEPTION select: received CDP message during navigation");
                match msg {
                    ServerMessage::NewConnection { reply_tx: new_tx } => {
                        let pid = ctx.create_page();
                        let sid = format!("{}-session", pid);
                        ctx.sessions.insert(sid.clone(), pid.clone());
                        let _ = new_tx.send(json!({"__init": true, "pageId": pid, "sessionId": sid}).to_string());
                    }
                    ServerMessage::Cdp(msg) => {
                        if msg.text.contains("Fetch.") {
                            handle_fetch_resolution(&msg.text, ctx, &msg.reply_tx, intercepted_paused);
                        } else {
                            crate::http::process_cdp_message(&msg.text, ctx, &msg.reply_tx).await;
                        }
                    }
                }
            }
        }
    }

    let mut page = page_back.expect("navigation task should return the page");

    let network_events: Vec<_> = page.network_events.drain(..).collect();
    let page_url = page.url_string();
    let page_id_for_events = page.id.clone();
    let reached_network_idle = page.lifecycle.is_network_idle();

    ctx.pages.push(page);

    let response = match navigate_result {
        Ok(()) => crate::types::CdpResponse::success(
            req.id,
            json!({"frameId": frame_id, "loaderId": loader_id}),
            req.session_id.clone(),
        ),
        Err(e) => crate::types::CdpResponse::error(req.id, -32000, e, req.session_id.clone()),
    };

    if let Ok(json) = serde_json::to_string(&response) {
        let _ = reply_tx.send(json);
    }

    emit_lifecycle_events(
        reply_tx,
        &session_for_events,
        &frame_id,
        &loader_id,
        &page_url,
        &page_id_for_events,
        &network_events,
        reached_network_idle,
    );
}

fn emit_lifecycle_events(
    reply_tx: &mpsc::UnboundedSender<String>,
    es: &Option<String>,
    frame_id: &str,
    loader_id: &str,
    page_url: &str,
    page_id_for_events: &str,
    network_events: &[obscura_browser::page::NetworkEvent],
    reached_network_idle: bool,
) {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();

    for event in [
        crate::types::CdpEvent {
            method: "Page.lifecycleEvent".into(),
            params: json!({"frameId": frame_id, "loaderId": loader_id, "name": "init", "timestamp": ts}),
            session_id: es.clone(),
        },
        crate::types::CdpEvent {
            method: "Runtime.executionContextsCleared".into(),
            params: json!({}),
            session_id: es.clone(),
        },
        crate::types::CdpEvent {
            method: "Page.frameNavigated".into(),
            params: json!({"frame": {"id": frame_id, "loaderId": loader_id, "url": page_url, "domainAndRegistry": "", "securityOrigin": page_url, "mimeType": "text/html", "adFrameStatus": {"adFrameType": "none"}}, "type": "Navigation"}),
            session_id: es.clone(),
        },
        crate::types::CdpEvent {
            method: "Runtime.executionContextCreated".into(),
            params: json!({"context": {"id": 2, "origin": page_url, "name": "", "uniqueId": format!("ctx-nav-{}", page_id_for_events), "auxData": {"isDefault": true, "type": "default", "frameId": frame_id}}}),
            session_id: es.clone(),
        },
        crate::types::CdpEvent {
            method: "Page.lifecycleEvent".into(),
            params: json!({"frameId": frame_id, "loaderId": loader_id, "name": "commit", "timestamp": ts}),
            session_id: es.clone(),
        },
    ] {
        if let Ok(json) = serde_json::to_string(&event) {
            let _ = reply_tx.send(json);
        }
    }

    for net_event in network_events {
        for event in [
            crate::types::CdpEvent {
                method: "Network.requestWillBeSent".into(),
                params: json!({"requestId": net_event.request_id, "loaderId": loader_id, "documentURL": page_url, "request": {"url": net_event.url, "method": net_event.method, "headers": net_event.headers}, "timestamp": net_event.timestamp, "wallTime": net_event.timestamp, "initiator": {"type": "other"}, "type": net_event.resource_type, "frameId": frame_id}),
                session_id: es.clone(),
            },
            crate::types::CdpEvent {
                method: "Network.responseReceived".into(),
                params: json!({"requestId": net_event.request_id, "loaderId": loader_id, "timestamp": net_event.timestamp, "type": net_event.resource_type, "response": {"url": net_event.url, "status": net_event.status, "statusText": "", "headers": &*net_event.response_headers, "mimeType": ""}, "frameId": frame_id}),
                session_id: es.clone(),
            },
            crate::types::CdpEvent {
                method: "Network.loadingFinished".into(),
                params: json!({"requestId": net_event.request_id, "timestamp": net_event.timestamp, "encodedDataLength": net_event.body_size}),
                session_id: es.clone(),
            },
        ] {
            if let Ok(json) = serde_json::to_string(&event) {
                let _ = reply_tx.send(json);
            }
        }
    }

    for event in [
        crate::types::CdpEvent {
            method: "Page.lifecycleEvent".into(),
            params: json!({"frameId": frame_id, "loaderId": loader_id, "name": "DOMContentLoaded", "timestamp": ts}),
            session_id: es.clone(),
        },
        crate::types::CdpEvent {
            method: "Page.domContentEventFired".into(),
            params: json!({"timestamp": ts}),
            session_id: es.clone(),
        },
        crate::types::CdpEvent {
            method: "Page.lifecycleEvent".into(),
            params: json!({"frameId": frame_id, "loaderId": loader_id, "name": "load", "timestamp": ts}),
            session_id: es.clone(),
        },
        crate::types::CdpEvent {
            method: "Page.loadEventFired".into(),
            params: json!({"timestamp": ts}),
            session_id: es.clone(),
        },
    ] {
        if let Ok(json) = serde_json::to_string(&event) {
            let _ = reply_tx.send(json);
        }
    }
    if reached_network_idle {
        let idle_event = crate::types::CdpEvent {
            method: "Page.lifecycleEvent".into(),
            params: json!({"frameId": frame_id, "loaderId": loader_id, "name": "networkIdle", "timestamp": ts}),
            session_id: es.clone(),
        };
        if let Ok(json) = serde_json::to_string(&idle_event) {
            let _ = reply_tx.send(json);
        }
    }
    let stop_event = crate::types::CdpEvent {
        method: "Page.frameStoppedLoading".into(),
        params: json!({"frameId": frame_id}),
        session_id: es.clone(),
    };
    if let Ok(json) = serde_json::to_string(&stop_event) {
        let _ = reply_tx.send(json);
    }
}

fn decode_base64(input: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(input)
        .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::decode_base64;

    #[test]
    fn decodes_basic_base64() {
        assert_eq!(decode_base64("SGVsbG8gV29ybGQ="), "Hello World");
    }

    #[test]
    fn decodes_empty_input() {
        assert_eq!(decode_base64(""), "");
    }
}
