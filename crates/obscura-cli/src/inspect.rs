use std::sync::Arc;

use obscura_browser::{BrowserContext, Page};

pub(crate) async fn run_inspect(url_str: &str, stealth: bool) -> anyhow::Result<()> {
    let context = Arc::new(BrowserContext::with_options(
        "inspect".to_string(),
        None,
        stealth,
    ));
    let mut page = Page::new("inspect-page".to_string(), context);

    let wait = obscura_browser::lifecycle::WaitUntil::Load;

    eprintln!("Inspecting {}...\n", url_str);

    let nav_start = std::time::Instant::now();
    let nav_result = page.navigate_with_wait(url_str, wait).await;
    let nav_time = nav_start.elapsed().as_millis();

    let nav_ok = nav_result.is_ok();
    if let Err(ref e) = nav_result {
        eprintln!("Navigation error: {}\n", e);
    }

    let title = page.title.clone();
    let url = page.url_string();
    let status = page
        .network_events
        .first()
        .map(|e| e.status)
        .unwrap_or(0);

    // Collect DOM stats
    let dom_stats = page
        .evaluate(&serde_json::json!(
            r#"(function() {
        var stats = {
            elements: document.querySelectorAll('*').length,
            scripts: document.querySelectorAll('script').length,
            styles: document.querySelectorAll('style,link[rel=stylesheet]').length,
            images: document.querySelectorAll('img').length,
            links: document.querySelectorAll('a').length,
            forms: document.querySelectorAll('form').length,
            inputs: document.querySelectorAll('input,textarea,select').length,
            iframes: document.querySelectorAll('iframe').length,
            hasBody: !!document.body,
            hasHead: !!document.head,
            hasTitle: !!document.title,
            doctype: !!document.doctype,
        };

        // Structure check
        stats.hasHeader = !!document.querySelector('header');
        stats.hasNav = !!document.querySelector('nav');
        stats.hasMain = !!document.querySelector('main');
        stats.hasFooter = !!document.querySelector('footer');
        stats.hasH1 = !!document.querySelector('h1');

        // Meta tags
        stats.metaViewport = !!document.querySelector('meta[name=viewport]');
        stats.metaDescription = document.querySelector('meta[name=description]')?.getAttribute('content')?.substring(0,100) || null;
        stats.metaOgTitle = document.querySelector('meta[property="og:title"]')?.getAttribute('content') || null;
        stats.charset = document.querySelector('meta[charset]')?.getAttribute('charset') || null;

        // JS errors
        stats.jsErrors = (globalThis.__obscura_errors || []).map(function(e) {
            return {msg: e.msg, src: e.src?.substring(0,80)};
        });

        // Console messages
        stats.consoleLogs = (globalThis.__obscura_console || []).slice(-10);

        // Accessibility basics
        stats.imagesWithoutAlt = document.querySelectorAll('img:not([alt])').length;
        stats.linksWithoutText = Array.from(document.querySelectorAll('a')).filter(function(a) {
            return !a.textContent.trim() && !a.querySelector('img');
        }).length;

        return JSON.stringify(stats);
    })()"#
        )
        .as_str()
        .unwrap_or("null")
        .to_string());

    let stats: serde_json::Value =
        serde_json::from_str(dom_stats.as_str().unwrap_or("{}")).unwrap_or(serde_json::json!({}));

    // Print report
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║  OBSCURA PAGE INSPECTOR                                ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    println!();

    // Navigation
    println!("── Navigation ──────────────────────────────────────────────");
    println!(
        "  Status:    {} {}",
        if nav_ok { "✅" } else { "❌" },
        if nav_ok {
            format!("OK ({}ms)", nav_time)
        } else {
            "FAILED".to_string()
        }
    );
    println!("  URL:       {}", url);
    println!("  HTTP:      {}", status);
    println!("  Title:     {}", title);
    println!();

    // DOM Structure
    println!("── DOM Structure ───────────────────────────────────────────");
    println!(
        "  Elements:  {}",
        stats["elements"].as_u64().unwrap_or(0)
    );
    println!("  Scripts:   {}", stats["scripts"].as_u64().unwrap_or(0));
    println!("  Styles:    {}", stats["styles"].as_u64().unwrap_or(0));
    println!("  Images:    {}", stats["images"].as_u64().unwrap_or(0));
    println!("  Links:     {}", stats["links"].as_u64().unwrap_or(0));
    println!("  Forms:     {}", stats["forms"].as_u64().unwrap_or(0));
    println!("  Inputs:    {}", stats["inputs"].as_u64().unwrap_or(0));
    println!("  Iframes:   {}", stats["iframes"].as_u64().unwrap_or(0));
    println!();

    // Semantic structure
    println!("── Semantic HTML ───────────────────────────────────────────");
    let checks = [
        ("doctype", "<!DOCTYPE>"),
        ("hasHead", "<head>"),
        ("hasBody", "<body>"),
        ("hasTitle", "<title>"),
        ("hasH1", "<h1>"),
        ("hasHeader", "<header>"),
        ("hasNav", "<nav>"),
        ("hasMain", "<main>"),
        ("hasFooter", "<footer>"),
    ];
    for (key, label) in checks {
        let ok = stats[key].as_bool().unwrap_or(false);
        println!("  {} {}", if ok { "✅" } else { "❌" }, label);
    }
    println!();

    // Meta tags
    println!("── Meta Tags ──────────────────────────────────────────────");
    println!(
        "  {} viewport",
        if stats["metaViewport"].as_bool().unwrap_or(false) {
            "✅"
        } else {
            "❌"
        }
    );
    println!(
        "  {} charset: {}",
        if stats["charset"].is_string() {
            "✅"
        } else {
            "❌"
        },
        stats["charset"].as_str().unwrap_or("missing")
    );
    if let Some(desc) = stats["metaDescription"].as_str() {
        println!("  ✅ description: {}...", &desc[..desc.len().min(60)]);
    } else {
        println!("  ❌ description: missing");
    }
    if let Some(og) = stats["metaOgTitle"].as_str() {
        println!("  ✅ og:title: {}", og);
    } else {
        println!("  ⚠️  og:title: missing");
    }
    println!();

    // Accessibility
    let imgs_no_alt = stats["imagesWithoutAlt"].as_u64().unwrap_or(0);
    let links_no_text = stats["linksWithoutText"].as_u64().unwrap_or(0);
    println!("── Accessibility ──────────────────────────────────────────");
    println!(
        "  {} Images without alt: {}",
        if imgs_no_alt == 0 { "✅" } else { "⚠️ " },
        imgs_no_alt
    );
    println!(
        "  {} Links without text: {}",
        if links_no_text == 0 { "✅" } else { "⚠️ " },
        links_no_text
    );
    println!();

    // JS Errors
    let errors = stats["jsErrors"].as_array();
    let error_count = errors.map(|e| e.len()).unwrap_or(0);
    println!("── JavaScript Errors ({}) ─────────────────────────────────", error_count);
    if error_count == 0 {
        println!("  ✅ No errors");
    } else if let Some(errs) = errors {
        for (i, err) in errs.iter().take(10).enumerate() {
            let msg = err["msg"].as_str().unwrap_or("?");
            let src = err["src"].as_str().unwrap_or("");
            println!("  {}. {}", i + 1, msg);
            if !src.is_empty() {
                println!("     └─ {}", src);
            }
        }
        if error_count > 10 {
            println!("  ... and {} more", error_count - 10);
        }
    }
    println!();

    // Summary
    let issues = error_count
        + imgs_no_alt as usize
        + links_no_text as usize
        + if !stats["hasH1"].as_bool().unwrap_or(false) {
            1
        } else {
            0
        }
        + if !stats["metaViewport"].as_bool().unwrap_or(false) {
            1
        } else {
            0
        };

    println!("── Summary ────────────────────────────────────────────────");
    println!(
        "  Load time: {}ms | Elements: {} | JS errors: {} | Issues: {}",
        nav_time,
        stats["elements"].as_u64().unwrap_or(0),
        error_count,
        issues
    );
    if issues == 0 {
        println!("  ✅ Page looks good!");
    } else {
        println!("  ⚠️  {} issue(s) found", issues);
    }

    Ok(())
}
