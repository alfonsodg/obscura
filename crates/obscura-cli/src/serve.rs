pub(crate) async fn run_multi_worker_serve(
    port: u16,
    workers: u16,
    proxy: Option<String>,
    stealth: bool,
) -> anyhow::Result<()> {
    use tokio::net::TcpListener;

    let exe = std::env::current_exe()?;
    let mut children = Vec::new();

    for i in 0..workers {
        let worker_port = port + 1 + i;
        let mut cmd = std::process::Command::new(&exe);
        cmd.arg("serve").arg("--port").arg(worker_port.to_string());
        if let Some(ref p) = proxy {
            cmd.arg("--proxy").arg(p);
        }
        if stealth {
            cmd.arg("--stealth");
        }
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::null());

        let child = cmd.spawn()?;
        tracing::info!("Worker {} on port {}", i + 1, worker_port);
        children.push(child);
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("Load balancer on port {}, {} workers", port, workers);

    let mut next_worker: u16 = 0;

    loop {
        let (client_stream, peer_addr) = listener.accept().await?;
        let worker_port = port + 1 + (next_worker % workers);
        next_worker = next_worker.wrapping_add(1);

        tracing::debug!("Routing {} to worker port {}", peer_addr, worker_port);

        let mut peek_buf = [0u8; 4];
        client_stream.peek(&mut peek_buf).await?;

        if &peek_buf == b"GET " {
            let mut full_peek = [0u8; 256];
            let n = client_stream.peek(&mut full_peek).await?;
            let request_line = String::from_utf8_lossy(&full_peek[..n]);

            if request_line.contains("/json") {
                let worker_addr = format!("127.0.0.1:{}", port + 1);
                if let Ok(mut worker_stream) = tokio::net::TcpStream::connect(&worker_addr).await {
                    tokio::spawn(async move {
                        let _ = tokio::io::copy_bidirectional(
                            &mut tokio::net::TcpStream::from_std(client_stream.into_std().unwrap()).unwrap(),
                            &mut worker_stream,
                        )
                        .await;
                    });
                }
                continue;
            }
        }

        let worker_addr = format!("127.0.0.1:{}", worker_port);
        tokio::spawn(async move {
            if let Ok(mut worker_stream) = tokio::net::TcpStream::connect(&worker_addr).await {
                let mut client = client_stream;
                let _ = tokio::io::copy_bidirectional(&mut client, &mut worker_stream).await;
            }
        });
    }
}
