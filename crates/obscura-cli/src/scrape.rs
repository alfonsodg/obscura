use std::sync::Arc;
use std::time::Instant;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command as TokioCommand;

pub(crate) async fn run_parallel_scrape(
    urls: Vec<String>,
    eval: Option<String>,
    concurrency: usize,
    format: &str,
) -> anyhow::Result<()> {
    let total = urls.len();
    let start = Instant::now();

    eprintln!("Scraping {} URLs with {} concurrent workers...", total, concurrency);

    let worker_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("obscura-worker")))
        .unwrap_or_else(|| std::path::PathBuf::from("obscura-worker"));

    if !worker_path.exists() {
        anyhow::bail!(
            "Worker binary not found at {}. Build with: cargo build --release",
            worker_path.display()
        );
    }

    let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));
    let eval = Arc::new(eval);
    let worker_path = Arc::new(worker_path);

    let mut handles = Vec::new();

    for (i, url) in urls.into_iter().enumerate() {
        let sem = semaphore.clone();
        let eval = eval.clone();
        let worker_path = worker_path.clone();

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            let task_start = Instant::now();

            let mut child = match TokioCommand::new(worker_path.as_ref())
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    return serde_json::json!({
                        "url": url,
                        "error": format!("Failed to spawn worker: {}", e),
                        "time_ms": task_start.elapsed().as_millis(),
                    });
                }
            };

            let stdin = child.stdin.as_mut().unwrap();
            let stdout = child.stdout.take().unwrap();
            let mut reader = BufReader::new(stdout);

            let nav_cmd = serde_json::json!({"cmd": "navigate", "url": url});
            let mut line = serde_json::to_string(&nav_cmd).unwrap();
            line.push('\n');
            if stdin.write_all(line.as_bytes()).await.is_err() {
                let _ = child.kill().await;
                return serde_json::json!({"url": url, "error": "Write failed"});
            }
            let _ = stdin.flush().await;

            let mut resp_line = String::new();
            if reader.read_line(&mut resp_line).await.is_err() {
                let _ = child.kill().await;
                return serde_json::json!({"url": url, "error": "Read failed"});
            }

            let nav_resp: serde_json::Value =
                serde_json::from_str(resp_line.trim()).unwrap_or(serde_json::json!({"ok": false}));

            if !nav_resp["ok"].as_bool().unwrap_or(false) {
                let _ = child.kill().await;
                return serde_json::json!({
                    "url": url,
                    "error": nav_resp["error"].as_str().unwrap_or("navigate failed"),
                    "time_ms": task_start.elapsed().as_millis(),
                });
            }

            let title = nav_resp["result"]["title"].as_str().unwrap_or("").to_string();

            let eval_result = if let Some(ref expr) = *eval {
                let eval_cmd = serde_json::json!({"cmd": "evaluate", "expression": expr});
                let mut line = serde_json::to_string(&eval_cmd).unwrap();
                line.push('\n');
                let _ = stdin.write_all(line.as_bytes()).await;
                let _ = stdin.flush().await;

                let mut resp_line = String::new();
                if reader.read_line(&mut resp_line).await.is_ok() {
                    let resp: serde_json::Value =
                        serde_json::from_str(resp_line.trim()).unwrap_or(serde_json::json!({"ok": false}));
                    resp["result"].clone()
                } else {
                    serde_json::Value::Null
                }
            } else {
                serde_json::Value::Null
            };

            let shutdown_cmd = serde_json::json!({"cmd": "shutdown"});
            let mut line = serde_json::to_string(&shutdown_cmd).unwrap();
            line.push('\n');
            let _ = stdin.write_all(line.as_bytes()).await;
            let _ = stdin.flush().await;
            let _ = child.wait().await;

            let elapsed = task_start.elapsed().as_millis();

            serde_json::json!({
                "url": url,
                "title": title,
                "eval": eval_result,
                "time_ms": elapsed,
                "worker": i,
            })
        });

        handles.push(handle);
    }

    let mut results = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(result) => results.push(result),
            Err(e) => results.push(serde_json::json!({"error": e.to_string()})),
        }
    }

    let total_time = start.elapsed();

    if format == "json" {
        let output = serde_json::json!({
            "total_urls": total,
            "concurrency": concurrency,
            "total_time_ms": total_time.as_millis(),
            "avg_time_ms": total_time.as_millis() as f64 / total as f64,
            "results": results,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        for r in &results {
            let url = r["url"].as_str().unwrap_or("?");
            let title = r["title"].as_str().unwrap_or("");
            let time = r["time_ms"].as_u64().unwrap_or(0);
            let eval = &r["eval"];
            if eval.is_null() {
                println!("{}ms\t{}\t{}", time, url, title);
            } else {
                println!("{}ms\t{}\t{}", time, url, eval);
            }
        }
        eprintln!(
            "\nTotal: {}ms for {} URLs ({} concurrent)",
            total_time.as_millis(),
            total,
            concurrency
        );
    }

    Ok(())
}
