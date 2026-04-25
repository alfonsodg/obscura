use url::Url;

use crate::page::Page;

impl Page {
    pub(crate) fn init_js(&mut self) {
        if self.js.is_some() {
            let url_str = self.url_string();
            let title = self.title.clone();
            let dom = self.dom.take();

            let js = self.js.as_mut().unwrap();
            js.set_url(&url_str);
            js.set_title(&title);

            if let Some(d) = dom {
                js.set_dom(d);
            }

            let _ = js.execute_script(
                "<reset>",
                "_cache.clear(); globalThis.__obscura_objects = {}; globalThis.__obscura_oid = 0; \
                 _iframeRegistry.length = 0; globalThis.length = 0; \
                 globalThis._formValues = {}; globalThis._formChecked = {}; \
                 globalThis._eventRegistry = {}; \
                 globalThis.document = new Document(+_dom('document_node_id'));",
            );

            return;
        }

        let mut rt = obscura_js::runtime::ObscuraJsRuntime::with_base_url(&self.url_string());
        rt.set_url(&self.url_string());
        rt.set_title(&self.title);

        #[cfg(feature = "stealth")]
        if self.stealth_client.is_some() {
            rt.set_user_agent(obscura_net::STEALTH_USER_AGENT);
        } else if let Ok(ua) = self.http_client.user_agent.try_read() {
            rt.set_user_agent(&ua);
        }
        #[cfg(not(feature = "stealth"))]
        if let Ok(ua) = self.http_client.user_agent.try_read() {
            rt.set_user_agent(&ua);
        }

        rt.set_cookie_jar(self.context.cookie_jar.clone());
        rt.set_http_client(self.http_client.clone());

        if let Some(tx) = &self.intercept_tx {
            rt.set_intercept_tx(tx.clone());
        }

        if let Some(dom) = self.dom.take() {
            rt.set_dom(dom);
        }

        self.js = Some(rt);
    }

    pub(crate) async fn execute_scripts(&mut self) {
        tracing::info!("execute_scripts called, js runtime exists: {}", self.js.is_some());

        #[derive(Debug)]
        struct ScriptInfo {
            src: Option<String>,
            inline: String,
            is_defer: bool,
            is_async: bool,
            is_module: bool,
        }

        let all_scripts = match &self.js {
            Some(js) => js
                .with_dom(|dom| {
                    let script_ids = dom.query_selector_all("script").unwrap_or_default();
                    let mut scripts = Vec::new();

                    for sid in script_ids {
                        if let Some(node) = dom.get_node(sid) {
                            let src = node.get_attribute("src").map(|s| s.to_string());
                            let script_type = node.get_attribute("type").unwrap_or("").to_string();
                            let is_defer = node.get_attribute("defer").is_some();
                            let is_async = node.get_attribute("async").is_some();
                            let is_module = script_type == "module";

                            if !script_type.is_empty()
                                && script_type != "text/javascript"
                                && script_type != "application/javascript"
                                && script_type != "module"
                            {
                                continue;
                            }

                            let inline_code = if src.is_none() {
                                dom.text_content(sid)
                            } else {
                                String::new()
                            };

                            if src.is_some() || !inline_code.trim().is_empty() {
                                scripts.push(ScriptInfo {
                                    src,
                                    inline: inline_code,
                                    is_defer,
                                    is_async,
                                    is_module,
                                });
                            }
                        }
                    }
                    scripts
                })
                .unwrap_or_default(),
            None => return,
        };

        let mut regular = Vec::new();
        let mut deferred = Vec::new();
        let mut async_scripts = Vec::new();
        let mut module_scripts = Vec::new();

        for script in all_scripts {
            if script.is_module {
                module_scripts.push(script);
                continue;
            }
            if script.is_defer {
                deferred.push(script);
            } else if script.is_async {
                async_scripts.push(script);
            } else {
                regular.push(script);
            }
        }

        let scripts = regular;

        tracing::info!(
            "Found {} regular + {} deferred + {} async scripts",
            scripts.len(),
            deferred.len(),
            async_scripts.len()
        );
        let all_to_execute: Vec<ScriptInfo> = scripts
            .into_iter()
            .chain(deferred.into_iter())
            .chain(async_scripts.into_iter())
            .collect();

        let mut resolved: Vec<(usize, String)> = Vec::new();
        let mut fetch_tasks: Vec<(usize, String)> = Vec::new();

        for (i, script) in all_to_execute.iter().enumerate() {
            if let Some(src_url) = &script.src {
                let full_url = if src_url.starts_with("http://") || src_url.starts_with("https://") {
                    src_url.clone()
                } else if let Some(base) = &self.url {
                    base.join(src_url)
                        .map(|u| u.to_string())
                        .unwrap_or_else(|_| src_url.clone())
                } else {
                    src_url.clone()
                };

                if self.should_block_url(&full_url) {
                    tracing::info!("Blocked script by interception: {}", full_url);
                    continue;
                }
                resolved.push((i, full_url.clone()));
                fetch_tasks.push((i, full_url));
            }
        }

        let client = self.http_client.clone();
        let fetch_futures: Vec<_> = fetch_tasks
            .iter()
            .map(|(idx, url)| {
                let client = client.clone();
                let url = url.clone();
                let idx = *idx;
                async move {
                    let parsed = Url::parse(&url).unwrap_or_else(|_| Url::parse("about:blank").unwrap());
                    match client.fetch(&parsed).await {
                        Ok(resp) => Some((idx, url, resp)),
                        Err(e) => {
                            tracing::warn!("Failed to fetch script {}: {}", url, e);
                            None
                        }
                    }
                }
            })
            .collect();

        let fetch_results = futures::future::join_all(fetch_futures).await;

        let mut fetched: std::collections::HashMap<usize, (String, String, obscura_net::Response)> =
            std::collections::HashMap::new();
        for result in fetch_results {
            if let Some((idx, url, resp)) = result {
                let code = String::from_utf8_lossy(&resp.body).to_string();
                fetched.insert(idx, (url, code, resp));
            }
        }

        for (i, script) in all_to_execute.iter().enumerate() {
            if script.src.is_some() {
                if let Some((url, code, resp)) = fetched.remove(&i) {
                    tracing::info!("Executing script ({} bytes): {}", code.len(), url);
                    self.record_network_event(&url, "GET", "Script", resp.status, &resp.headers, resp.body.len());
                    if let Some(js) = &mut self.js {
                        if let Err(e) = js.execute_script_guarded(&url, &code) {
                            tracing::warn!("Script error ({}): {}", url, e);
                        }
                    }
                }
            } else if !script.inline.is_empty() {
                if let Some(js) = &mut self.js {
                    if let Err(e) = js.execute_script_guarded("<inline>", &script.inline) {
                        tracing::warn!("Inline script error: {}", e);
                    }
                }
            }
        }

        for module_script in &module_scripts {
            if let Some(ref src) = module_script.src {
                let full_url = if src.starts_with("http://") || src.starts_with("https://") {
                    src.clone()
                } else if let Some(base) = &self.url {
                    base.join(src).map(|u| u.to_string()).unwrap_or_else(|_| src.clone())
                } else {
                    src.clone()
                };

                tracing::info!("Loading ES module: {}", full_url);
                if let Some(js) = &mut self.js {
                    match js.load_module(&full_url).await {
                        Ok(()) => {
                            tracing::info!("ES module loaded: {}", full_url);
                            self.record_network_event(
                                &full_url,
                                "GET",
                                "Script",
                                200,
                                &std::collections::HashMap::new(),
                                0,
                            );
                        }
                        Err(e) => {
                            tracing::warn!("ES module error ({}): {}", full_url, e);
                        }
                    }
                }
            } else if !module_script.inline.is_empty() {
                let base = self.url_string();
                if let Some(js) = &mut self.js {
                    if let Err(e) = js.load_inline_module(&module_script.inline, &base).await {
                        tracing::warn!("Inline ES module error: {}", e);
                    }
                }
            }
        }

        if let Some(js) = &mut self.js {
            let _ = js.execute_script(
                "<load-events>",
                "if (typeof window.onload === 'function') { try { window.onload(); } catch(e) {} }\n\
                 try { document.dispatchEvent(new Event('DOMContentLoaded')); } catch(e) {}\n\
                 try { window.dispatchEvent(new Event('load')); } catch(e) {}",
            );
        }

        if let Some(js) = &mut self.js {
            let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_millis(500);
            let mut idle_count = 0u32;
            loop {
                let result = tokio::time::timeout(tokio::time::Duration::from_millis(10), js.run_event_loop()).await;

                match result {
                    Ok(Ok(())) => {
                        if self.http_client.active_requests() == 0 {
                            idle_count += 1;
                            if idle_count >= 2 {
                                break;
                            }
                            tokio::task::yield_now().await;
                        } else {
                            idle_count = 0;
                            tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
                        }
                    }
                    Ok(Err(_)) => break,
                    Err(_) => {
                        idle_count = 0;
                        if tokio::time::Instant::now() >= deadline {
                            break;
                        }
                    }
                }
            }
        }
    }

    pub fn evaluate(&mut self, expression: &str) -> serde_json::Value {
        if let Some(js) = &mut self.js {
            match js.evaluate(expression) {
                Ok(val) => val,
                Err(e) => {
                    tracing::debug!("JS eval error for '{}': {}", &expression[..expression.len().min(80)], e);
                    serde_json::Value::Null
                }
            }
        } else {
            match expression.trim() {
                "document.title" => serde_json::Value::String(self.title.clone()),
                "document.URL" | "document.location.href" | "window.location.href" => {
                    serde_json::Value::String(self.url_string())
                }
                _ => serde_json::Value::Null,
            }
        }
    }

    pub fn evaluate_for_cdp(
        &mut self,
        expression: &str,
        return_by_value: bool,
    ) -> obscura_js::runtime::RemoteObjectInfo {
        if let Some(js) = &mut self.js {
            match js.evaluate_for_cdp(expression, return_by_value) {
                Ok(info) => info,
                Err(e) => {
                    tracing::debug!("evaluate_for_cdp error: {}", e);
                    obscura_js::runtime::RemoteObjectInfo {
                        js_type: "undefined".into(),
                        subtype: None,
                        class_name: String::new(),
                        description: String::new(),
                        object_id: None,
                        value: None,
                    }
                }
            }
        } else {
            let val = self.evaluate(expression);
            obscura_js::runtime::RemoteObjectInfo {
                js_type: match &val {
                    serde_json::Value::String(_) => "string".into(),
                    serde_json::Value::Number(_) => "number".into(),
                    serde_json::Value::Bool(_) => "boolean".into(),
                    _ => "undefined".into(),
                },
                subtype: None,
                class_name: String::new(),
                description: String::new(),
                object_id: None,
                value: Some(val),
            }
        }
    }

    pub async fn call_function_on_for_cdp(
        &mut self,
        function_declaration: &str,
        object_id: Option<&str>,
        args: &[serde_json::Value],
        return_by_value: bool,
        await_promise: bool,
    ) -> obscura_js::runtime::RemoteObjectInfo {
        if let Some(js) = &mut self.js {
            match js
                .call_function_on_for_cdp(function_declaration, object_id, args, return_by_value, await_promise)
                .await
            {
                Ok(info) => info,
                Err(e) => {
                    tracing::debug!("callFunctionOn error: {}", e);
                    obscura_js::runtime::RemoteObjectInfo {
                        js_type: "undefined".into(),
                        subtype: None,
                        class_name: String::new(),
                        description: String::new(),
                        object_id: None,
                        value: None,
                    }
                }
            }
        } else {
            obscura_js::runtime::RemoteObjectInfo {
                js_type: "undefined".into(),
                subtype: None,
                class_name: String::new(),
                description: String::new(),
                object_id: None,
                value: None,
            }
        }
    }

    pub fn execute_preload_script(&mut self, source: &str) -> Result<(), String> {
        if let Some(js) = &mut self.js {
            js.execute_script("<preload>", source)
        } else {
            Err("No JS runtime".to_string())
        }
    }

    pub fn suspend_js(&mut self) {
        if let Some(js) = &self.js {
            if let Some(dom) = js.take_dom() {
                self.dom = Some(dom);
            }
        }
        self.js = None;
    }

    pub fn resume_js(&mut self) {
        if self.js.is_some() {
            return;
        }
        self.init_js();
    }

    pub fn has_js(&self) -> bool {
        self.js.is_some()
    }

    pub fn release_object(&mut self, object_id: &str) {
        if let Some(js) = &mut self.js {
            js.release_object(object_id);
        }
    }

    pub fn release_object_group(&mut self) {
        if let Some(js) = &mut self.js {
            js.release_object_group();
        }
    }

    pub fn set_blocked_urls(&mut self, patterns: Vec<String>) {
        if let Some(js) = &self.js {
            js.set_blocked_urls(patterns);
        }
    }

    pub fn take_pending_navigation(&self) -> Option<(String, String, String)> {
        if let Some(js) = &self.js {
            js.take_pending_navigation()
        } else {
            None
        }
    }

    pub fn set_intercept_tx(&mut self, tx: tokio::sync::mpsc::UnboundedSender<obscura_js::ops::InterceptedRequest>) {
        self.intercept_tx = Some(tx.clone());
        if let Some(js) = &self.js {
            js.set_intercept_tx(tx);
        }
    }
}
