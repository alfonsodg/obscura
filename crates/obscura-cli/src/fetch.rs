use std::sync::Arc;

use obscura_browser::{BrowserContext, Page};

use crate::DumpFormat;

pub(crate) async fn run_fetch(
    url_str: &str,
    dump: DumpFormat,
    selector: Option<String>,
    wait_secs: u64,
    wait_until: &str,
    user_agent: Option<String>,
    stealth: bool,
    eval: Option<String>,
    quiet: bool,
) -> anyhow::Result<()> {
    let context = Arc::new(BrowserContext::with_options("fetch".to_string(), None, stealth));
    let mut page = Page::new("fetch-page".to_string(), context);

    if let Some(ref ua) = user_agent {
        page.http_client.set_user_agent(ua).await;
    }

    let wait_condition = obscura_browser::lifecycle::WaitUntil::from_str(wait_until);

    if !quiet {
        eprintln!("Fetching {}...", url_str);
    }

    page.navigate_with_wait(url_str, wait_condition)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to navigate to {}: {}", url_str, e))?;

    if !quiet {
        eprintln!("Page loaded: {} - \"{}\"", page.url_string(), page.title);
    }

    if let Some(ref sel) = selector {
        let found = wait_for_selector(&mut page, sel, wait_secs).await;
        if !found {
            eprintln!("Warning: selector '{}' not found after {}s", sel, wait_secs);
        }
    }

    if let Some(ref expr) = eval {
        let result = page.evaluate(expr);
        match result {
            serde_json::Value::String(s) => println!("{}", s),
            serde_json::Value::Null => println!("null"),
            other => println!("{}", other),
        }
        return Ok(());
    }

    match dump {
        DumpFormat::Html => {
            dump_html(&page);
        }
        DumpFormat::Text => {
            dump_text(&mut page);
        }
        DumpFormat::Links => {
            dump_links(&page);
        }
    }

    Ok(())
}

async fn wait_for_selector(page: &mut Page, selector: &str, timeout_secs: u64) -> bool {
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(timeout_secs);
    loop {
        let found = page
            .with_dom(|dom| dom.query_selector(selector).ok().flatten().is_some())
            .unwrap_or(false);

        if found {
            return true;
        }

        if tokio::time::Instant::now() >= deadline {
            return false;
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}

fn dump_html(page: &Page) {
    page.with_dom(|dom| {
        if let Ok(Some(html_node)) = dom.query_selector("html") {
            let html = dom.outer_html(html_node);
            println!("<!DOCTYPE html>");
            println!("{}", html);
        } else {
            let doc = dom.document();
            let html = dom.inner_html(doc);
            println!("{}", html);
        }
    });
}

fn dump_text(page: &mut Page) {
    page.with_dom(|dom| {
        if let Ok(Some(body)) = dom.query_selector("body") {
            let text = extract_readable_text(dom, body);
            println!("{}", text.trim());
        }
    });
}

fn extract_readable_text(dom: &obscura_dom::DomTree, node_id: obscura_dom::NodeId) -> String {
    use obscura_dom::NodeData;

    let mut result = String::new();
    let node = match dom.get_node(node_id) {
        Some(n) => n,
        None => return result,
    };

    match &node.data {
        NodeData::Text { contents } => {
            let trimmed = contents.trim();
            if !trimmed.is_empty() {
                result.push_str(trimmed);
            }
        }
        NodeData::Element { name, .. } => {
            let tag = name.local.as_ref();
            let is_block = matches!(
                tag,
                "div"
                    | "p"
                    | "h1"
                    | "h2"
                    | "h3"
                    | "h4"
                    | "h5"
                    | "h6"
                    | "li"
                    | "tr"
                    | "br"
                    | "hr"
                    | "blockquote"
                    | "pre"
                    | "section"
                    | "article"
                    | "header"
                    | "footer"
                    | "nav"
                    | "main"
                    | "aside"
                    | "figure"
                    | "figcaption"
                    | "table"
                    | "thead"
                    | "tbody"
                    | "tfoot"
                    | "dl"
                    | "dt"
                    | "dd"
                    | "ul"
                    | "ol"
            );

            if tag == "script" || tag == "style" {
                return result;
            }

            if is_block {
                result.push('\n');
            }

            for child_id in dom.children(node_id) {
                result.push_str(&extract_readable_text(dom, child_id));
            }

            if is_block {
                result.push('\n');
            }
        }
        _ => {
            for child_id in dom.children(node_id) {
                result.push_str(&extract_readable_text(dom, child_id));
            }
        }
    }

    result
}

fn dump_links(page: &Page) {
    let base_url = page.url.clone();
    page.with_dom(|dom| {
        let links = dom.query_selector_all("a").unwrap_or_default();
        for link_id in links {
            if let Some(node) = dom.get_node(link_id) {
                let href = node.get_attribute("href").unwrap_or_default().to_string();
                let text = dom.text_content(link_id);
                let text = text.trim();

                let full_url = if href.starts_with("http://") || href.starts_with("https://") {
                    href.clone()
                } else if let Some(ref base) = base_url {
                    base.join(&href).map(|u| u.to_string()).unwrap_or(href.clone())
                } else {
                    href.clone()
                };

                if !full_url.is_empty() {
                    if text.is_empty() {
                        println!("{}", full_url);
                    } else {
                        println!("{}\t{}", full_url, text);
                    }
                }
            }
        }
    });
}
