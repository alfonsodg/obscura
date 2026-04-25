use std::collections::HashMap;
use std::net::SocketAddr;

use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

use crate::dispatch::CdpContext;
use crate::types::CdpRequest;

pub(crate) struct CdpMessage {
    pub text: String,
    pub reply_tx: mpsc::UnboundedSender<String>,
}

pub(crate) enum ServerMessage {
    Cdp(CdpMessage),
    NewConnection {
        reply_tx: mpsc::UnboundedSender<String>,
    },
}

pub async fn start(port: u16) -> anyhow::Result<()> {
    start_with_options(port, None).await
}

pub async fn start_with_options(port: u16, proxy: Option<String>) -> anyhow::Result<()> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(&addr).await?;

    info!("Obscura CDP server listening on ws://127.0.0.1:{}", port);
    info!(
        "DevTools endpoint: ws://127.0.0.1:{}/devtools/browser",
        port
    );

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let (msg_tx, msg_rx) = mpsc::unbounded_channel::<ServerMessage>();

            let _processor_handle = tokio::task::spawn_local(cdp_processor(msg_rx, proxy));

            loop {
                match listener.accept().await {
                    Ok((stream, peer_addr)) => {
                        info!("New connection from {}", peer_addr);
                        let tx = msg_tx.clone();
                        tokio::task::spawn_local(async move {
                            if let Err(e) = handle_connection(stream, port, tx).await {
                                if !format!("{}", e).contains("close") {
                                    error!("Connection error from {}: {}", peer_addr, e);
                                }
                            }
                        });
                    }
                    Err(e) => error!("Accept error: {}", e),
                }
            }
        })
        .await
}

async fn cdp_processor(
    mut rx: mpsc::UnboundedReceiver<ServerMessage>,
    proxy: Option<String>,
) {
    let mut ctx = CdpContext::new_with_proxy(proxy);
    let (itx, irx) = mpsc::unbounded_channel::<obscura_js::ops::InterceptedRequest>();
    ctx.intercept_tx = Some(itx);
    let mut intercept_rx: Option<mpsc::UnboundedReceiver<obscura_js::ops::InterceptedRequest>> = Some(irx);
    let mut intercepted_paused: HashMap<String, tokio::sync::oneshot::Sender<obscura_js::ops::InterceptResolution>> = HashMap::new();

    while let Some(msg) = rx.recv().await {
        match msg {
            ServerMessage::NewConnection { reply_tx } => {
                let _ = reply_tx.send(
                    json!({"__init": true})
                        .to_string(),
                );
            }
            ServerMessage::Cdp(cdp_msg) => {
                let is_navigation = cdp_msg.text.contains("Page.navigate");
                let has_interception = ctx.fetch_intercept.enabled;

                if is_navigation && has_interception {
                    crate::interception::process_with_interception(
                        &cdp_msg.text, &mut ctx, &cdp_msg.reply_tx, &mut rx,
                        &mut intercept_rx, &mut intercepted_paused,
                    ).await;
                } else {
                    if cdp_msg.text.contains("Fetch.") {
                        crate::interception::handle_fetch_resolution(&cdp_msg.text, &mut ctx, &cdp_msg.reply_tx, &mut intercepted_paused);
                    }
                    crate::http::process_cdp_message(&cdp_msg.text, &mut ctx, &cdp_msg.reply_tx).await;
                }
            }
        }
    }
}

async fn handle_connection(
    stream: TcpStream,
    port: u16,
    msg_tx: mpsc::UnboundedSender<ServerMessage>,
) -> anyhow::Result<()> {
    let mut buf = [0u8; 4];
    stream.peek(&mut buf).await?;

    if &buf == b"GET " {
        let mut peek_buf = [0u8; 1024];
        let n = stream.peek(&mut peek_buf).await?;
        let line = String::from_utf8_lossy(&peek_buf[..n]);

        if line.contains("/json/version") {
            return crate::http::handle_http_json(stream, port, "version").await;
        } else if line.contains("/json/list") || line.contains("/json\r\n") || line.contains("/json HTTP") {
            return crate::http::handle_http_json(stream, port, "list").await;
        } else if line.contains("/json/protocol") {
            return crate::http::handle_http_json(stream, port, "protocol").await;
        } else if line.contains("/health") {
            return crate::http::handle_health(stream).await;
        }
    }

    let ws_stream = tokio_tungstenite::accept_async(stream).await?;
    info!("WebSocket connected");
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    let (reply_tx, mut reply_rx) = mpsc::unbounded_channel::<String>();

    let _ = msg_tx.send(ServerMessage::NewConnection {
        reply_tx: reply_tx.clone(),
    });
    if let Some(init_msg) = reply_rx.recv().await {
        tracing::debug!("Connection init: {}", &init_msg[..init_msg.len().min(100)]);
    }

    let send_task = tokio::task::spawn_local(async move {
        while let Some(msg) = reply_rx.recv().await {
            if msg.contains("\"__init\"") {
                continue;
            }
            if ws_sender.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    while let Some(msg) = ws_receiver.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                warn!("WS read error: {}", e);
                break;
            }
        };

        match msg {
            Message::Text(text) => {
                if text.contains("\"Browser.close\"") {
                    if let Ok(req) = serde_json::from_str::<CdpRequest>(&text) {
                        let resp = crate::types::CdpResponse::success(req.id, json!({}), None);
                        if let Ok(json) = serde_json::to_string(&resp) {
                            let _ = reply_tx.send(json);
                        }
                    }
                    break;
                }

                if let Some(resp) = crate::http::fast_path_response(&text) {
                    let _ = reply_tx.send(resp);
                } else {
                    let _ = msg_tx.send(ServerMessage::Cdp(CdpMessage {
                        text: text.to_string(),
                        reply_tx: reply_tx.clone(),
                    }));
                }
            }
            Message::Close(_) => {
                info!("WS closed by client");
                break;
            }
            _ => {}
        }
    }

    send_task.abort();
    Ok(())
}
