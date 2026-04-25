use std::sync::Arc;

use obscura_dom::{parse_html, DomTree};
use obscura_js::runtime::ObscuraJsRuntime;
use obscura_net::{ObscuraHttpClient, ObscuraNetError, Response};
use url::Url;

use crate::context::BrowserContext;
use crate::lifecycle::LifecycleState;

#[cfg(feature = "stealth")]
use obscura_net::StealthHttpClient;

#[derive(Debug, Clone)]
pub struct NetworkEvent {
    pub request_id: String,
    pub url: String,
    pub method: String,
    pub resource_type: String,
    pub status: u16,
    pub headers: std::collections::HashMap<String, String>,
    pub response_headers: Arc<std::collections::HashMap<String, String>>,
    pub body_size: usize,
    pub timestamp: f64,
}

pub struct Page {
    pub id: String,
    pub frame_id: String,
    pub url: Option<Url>,
    pub dom: Option<DomTree>,
    pub js: Option<ObscuraJsRuntime>,
    pub lifecycle: LifecycleState,
    pub http_client: Arc<ObscuraHttpClient>,
    pub context: Arc<BrowserContext>,
    pub title: String,
    pub network_events: Vec<NetworkEvent>,
    network_event_counter: u32,
    pub intercept_enabled: bool,
    pub intercept_block_patterns: Vec<String>,
    pub(crate) intercept_tx: Option<tokio::sync::mpsc::UnboundedSender<obscura_js::ops::InterceptedRequest>>,
    #[cfg(feature = "stealth")]
    pub stealth_client: Option<Arc<StealthHttpClient>>,
}

impl Page {
    pub fn new(id: String, context: Arc<BrowserContext>) -> Self {
        let http_client = context.http_client.clone();
        let frame_id = format!("{}.1", id);
        #[cfg(feature = "stealth")]
        let stealth_client = if context.stealth {
            Some(Arc::new(StealthHttpClient::new(context.cookie_jar.clone())))
        } else {
            None
        };

        Page {
            id,
            frame_id,
            url: None,
            dom: None,
            js: None,
            lifecycle: LifecycleState::Idle,
            http_client,
            context,
            title: String::new(),
            network_events: Vec::new(),
            network_event_counter: 0,
            intercept_enabled: false,
            intercept_block_patterns: Vec::new(),
            intercept_tx: None,
            #[cfg(feature = "stealth")]
            stealth_client,
        }
    }

    pub(crate) fn should_block_url(&self, url: &str) -> bool {
        if !self.intercept_enabled || self.intercept_block_patterns.is_empty() {
            return false;
        }
        for pattern in &self.intercept_block_patterns {
            if pattern == "*" {
                return true;
            }
            if pattern.starts_with('*') && pattern.ends_with('*') {
                if url.contains(&pattern[1..pattern.len() - 1]) {
                    return true;
                }
            } else if pattern.starts_with('*') {
                if url.ends_with(&pattern[1..]) {
                    return true;
                }
            } else if pattern.ends_with('*') {
                if url.starts_with(&pattern[..pattern.len() - 1]) {
                    return true;
                }
            } else if url.contains(pattern) {
                return true;
            }
        }
        false
    }

    async fn do_fetch(&self, url: &Url) -> Result<Response, ObscuraNetError> {
        #[cfg(feature = "stealth")]
        if let Some(ref stealth) = self.stealth_client {
            return stealth.fetch(url).await;
        }
        self.http_client.fetch(url).await
    }

    pub async fn navigate(&mut self, url_str: &str) -> Result<(), PageError> {
        self.navigate_with_wait(url_str, crate::lifecycle::WaitUntil::Load)
            .await
    }

    pub async fn navigate_with_wait(
        &mut self,
        url_str: &str,
        wait_until: crate::lifecycle::WaitUntil,
    ) -> Result<(), PageError> {
        self.navigate_with_wait_post(url_str, wait_until, "GET", "").await
    }

    pub async fn navigate_with_wait_post(
        &mut self,
        url_str: &str,
        wait_until: crate::lifecycle::WaitUntil,
        method: &str,
        body: &str,
    ) -> Result<(), PageError> {
        let mut current_url = url_str.to_string();
        let mut current_method = method.to_string();
        let mut current_body = body.to_string();
        for _chain in 0..10 {
            self.navigate_single(&current_url, wait_until, &current_method, &current_body)
                .await?;
            if let Some((next_url, next_method, next_body)) = self.take_pending_navigation() {
                tracing::info!(
                    "JS-triggered navigation chain: {} {} -> {}",
                    current_method,
                    current_url,
                    next_url
                );
                current_url = next_url;
                current_method = next_method;
                current_body = next_body;
                continue;
            }
            break;
        }
        Ok(())
    }

    async fn navigate_single(
        &mut self,
        url_str: &str,
        wait_until: crate::lifecycle::WaitUntil,
        method: &str,
        body: &str,
    ) -> Result<(), PageError> {
        let url = Url::parse(url_str).map_err(|e| PageError::InvalidUrl(e.to_string()))?;

        self.lifecycle = LifecycleState::Loading;
        self.url = Some(url.clone());
        self.network_events.clear();

        if self.context.obey_robots {
            if let Some(domain) = url.host_str() {
                if self.context.robots_cache.is_allowed(domain, "/robots.txt") {
                    let robots_url = format!("{}://{}/robots.txt", url.scheme(), domain);
                    if let Ok(robots_url) = Url::parse(&robots_url) {
                        if let Ok(resp) = self.http_client.fetch(&robots_url).await {
                            if resp.status == 200 {
                                let body = String::from_utf8_lossy(&resp.body);
                                self.context
                                    .robots_cache
                                    .parse_and_store(domain, &body, &self.context.user_agent);
                            }
                        }
                    }
                }

                if !self.context.robots_cache.is_allowed(domain, url.path()) {
                    self.lifecycle = LifecycleState::Failed;
                    return Err(PageError::NetworkError(format!("Blocked by robots.txt: {}", url)));
                }
            }
        }

        let response = if method == "POST" {
            self.http_client.post_form(&url, body).await
        } else {
            self.do_fetch(&url).await
        }
        .map_err(|e| {
            self.lifecycle = LifecycleState::Failed;
            PageError::NetworkError(e.to_string())
        })?;

        self.record_network_event(
            url.as_str(),
            "GET",
            "Document",
            response.status,
            &response.headers,
            response.body.len(),
        );

        if !response.redirected_from.is_empty() {
            self.url = Some(response.url.clone());
        }

        let body_text = String::from_utf8_lossy(&response.body).to_string();
        let dom = parse_html(&body_text);

        self.title = dom
            .query_selector("title")
            .ok()
            .flatten()
            .map(|title_id| dom.text_content(title_id))
            .unwrap_or_default();

        let stylesheet_urls: Vec<String> = dom
            .query_selector_all("link")
            .unwrap_or_default()
            .iter()
            .filter_map(|&nid| {
                let node = dom.get_node(nid)?;
                let rel = node.get_attribute("rel")?;
                if rel.to_lowercase() != "stylesheet" {
                    return None;
                }
                node.get_attribute("href").map(|s| s.to_string())
            })
            .collect();

        let mut css_fetch_urls: Vec<String> = Vec::new();
        for href in &stylesheet_urls {
            let full_url = if href.starts_with("http://") || href.starts_with("https://") {
                href.clone()
            } else if let Some(base) = &self.url {
                base.join(href).map(|u| u.to_string()).unwrap_or_else(|_| href.clone())
            } else {
                href.clone()
            };
            if self.should_block_url(&full_url) {
                tracing::info!("Blocked stylesheet by interception: {}", full_url);
                continue;
            }
            css_fetch_urls.push(full_url);
        }

        let client = self.http_client.clone();
        let css_futures: Vec<_> = css_fetch_urls
            .iter()
            .map(|full_url| {
                let client = client.clone();
                let url_str = full_url.clone();
                async move {
                    let parsed = Url::parse(&url_str).unwrap_or_else(|_| Url::parse("about:blank").unwrap());
                    match client.fetch(&parsed).await {
                        Ok(resp) => Some((url_str, resp)),
                        Err(e) => {
                            tracing::debug!("Failed to fetch stylesheet {}: {}", url_str, e);
                            None
                        }
                    }
                }
            })
            .collect();

        let css_results = futures::future::join_all(css_futures).await;
        let mut css_sources = Vec::new();
        for (url_str, resp) in css_results.into_iter().flatten() {
            let css = String::from_utf8_lossy(&resp.body).to_string();
            self.record_network_event(
                &url_str,
                "GET",
                "Stylesheet",
                resp.status,
                &resp.headers,
                resp.body.len(),
            );
            css_sources.push(css);
        }

        self.dom = Some(dom);
        self.lifecycle = LifecycleState::DomContentLoaded;

        if wait_until == crate::lifecycle::WaitUntil::DomContentLoaded {
            self.init_js();
            return Ok(());
        }

        self.init_js();

        if !css_sources.is_empty() {
            if let Some(js) = &mut self.js {
                let combined_css = css_sources.join("\n");
                let escaped = combined_css
                    .replace('\\', "\\\\")
                    .replace('`', "\\`")
                    .replace("${", "\\${");
                let code = format!("globalThis.__obscura_css = `{}`;", escaped);
                let _ = js.execute_script("<css>", &code);
            }
        }
        if let Some(js) = &mut self.js {
            let _ = js.execute_script("<iframe-load>",
                "(function() { var iframes = document.querySelectorAll('iframe[src]'); for (var i = 0; i < iframes.length; i++) { var src = iframes[i].getAttribute('src'); if (src && src !== 'about:blank') iframes[i]._loadIframeSrc(src); } })()");
        }

        self.execute_scripts().await;

        if let Some(js) = &mut self.js {
            if let Ok(new_title) = js.evaluate("document.title") {
                if let Some(t) = new_title.as_str() {
                    self.title = t.to_string();
                }
            }
        }

        self.lifecycle = LifecycleState::Loaded;

        if matches!(
            wait_until,
            crate::lifecycle::WaitUntil::NetworkIdle0 | crate::lifecycle::WaitUntil::NetworkIdle2
        ) {
            let threshold = match wait_until {
                crate::lifecycle::WaitUntil::NetworkIdle0 => 0,
                crate::lifecycle::WaitUntil::NetworkIdle2 => 2,
                _ => 0,
            };

            let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(5);
            let mut idle_since: Option<tokio::time::Instant> = None;

            loop {
                let active = self.http_client.active_requests();
                let now = tokio::time::Instant::now();

                if active <= threshold {
                    if idle_since.is_none() {
                        idle_since = Some(now);
                    }
                    if now.duration_since(idle_since.unwrap()) >= tokio::time::Duration::from_millis(500) {
                        break;
                    }
                } else {
                    idle_since = None;
                }

                if now >= deadline {
                    tracing::debug!("Network idle timeout reached with {} active requests", active);
                    break;
                }

                if let Some(js) = &mut self.js {
                    let _ = tokio::time::timeout(tokio::time::Duration::from_millis(50), js.run_event_loop()).await;
                } else {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            }

            self.lifecycle = LifecycleState::NetworkIdle;
        }

        Ok(())
    }

    pub fn navigate_blank(&mut self) {
        self.js = None;
        self.url = Some(Url::parse("about:blank").unwrap());
        self.dom = Some(parse_html("<!DOCTYPE html><html><head></head><body></body></html>"));
        self.title = String::new();
        self.lifecycle = LifecycleState::Loaded;
    }

    pub fn url_string(&self) -> String {
        self.url
            .as_ref()
            .map(|u| u.to_string())
            .unwrap_or_else(|| "about:blank".to_string())
    }

    pub fn with_dom<R>(&self, f: impl FnOnce(&DomTree) -> R) -> Option<R> {
        if let Some(js) = &self.js {
            return js.with_dom(f);
        }
        self.dom.as_ref().map(f)
    }

    pub fn dom(&self) -> Option<&DomTree> {
        self.dom.as_ref()
    }

    pub(crate) fn record_network_event(
        &mut self,
        url: &str,
        method: &str,
        resource_type: &str,
        status: u16,
        response_headers: &std::collections::HashMap<String, String>,
        body_size: usize,
    ) {
        self.network_event_counter += 1;
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        self.network_events.push(NetworkEvent {
            request_id: format!("{}.{}", self.id, self.network_event_counter),
            url: url.to_string(),
            method: method.to_string(),
            resource_type: resource_type.to_string(),
            status,
            headers: std::collections::HashMap::new(),
            response_headers: Arc::new(response_headers.clone()),
            body_size,
            timestamp,
        });
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PageError {
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Parse error: {0}")]
    ParseError(String),
}

impl From<ObscuraNetError> for PageError {
    fn from(e: ObscuraNetError) -> Self {
        PageError::NetworkError(e.to_string())
    }
}
