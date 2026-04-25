mod fetch;
mod scrape;
mod serve;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "obscura",
    about = "Obscura - A lightweight headless browser for web scraping and automation"
)]
struct Args {
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Option<Command>,

    #[arg(short, long, default_value_t = 9222)]
    port: u16,

    #[arg(long)]
    proxy: Option<String>,

    #[arg(long)]
    obey_robots: bool,

    #[arg(long)]
    user_agent: Option<String>,
}

#[derive(Subcommand)]
enum Command {
    Serve {
        #[arg(short, long, default_value_t = 9222)]
        port: u16,

        #[arg(long)]
        proxy: Option<String>,

        #[arg(long)]
        user_agent: Option<String>,

        #[arg(long)]
        stealth: bool,

        #[arg(long, default_value_t = 1)]
        workers: u16,
    },

    Fetch {
        url: String,

        #[arg(long, default_value = "html")]
        dump: DumpFormat,

        #[arg(long)]
        selector: Option<String>,

        #[arg(long, default_value_t = 5)]
        wait: u64,

        #[arg(long, default_value = "load")]
        wait_until: String,

        #[arg(long)]
        user_agent: Option<String>,

        #[arg(long)]
        stealth: bool,

        #[arg(long, short)]
        eval: Option<String>,

        #[arg(long, short)]
        quiet: bool,
    },

    Scrape {
        urls: Vec<String>,

        #[arg(long, short)]
        eval: Option<String>,

        #[arg(long, default_value_t = 10)]
        concurrency: usize,

        #[arg(long, default_value = "json")]
        format: String,
    },
}

#[derive(Clone, Debug, clap::ValueEnum)]
enum DumpFormat {
    Html,
    Text,
    Links,
}

fn print_banner(port: u16) {
    println!(
        r#"
   ____  _                              
  / __ \| |                             
 | |  | | |__  ___  ___ _   _ _ __ __ _ 
 | |  | | '_ \/ __|/ __| | | | '__/ _` |
 | |__| | |_) \__ \ (__| |_| | | | (_| |
  \____/|_.__/|___/\___|\__,_|_|  \__,_|
                   
  Headless Browser v0.1.0
  CDP server: ws://127.0.0.1:{}/devtools/browser
"#,
        port
    );
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let filter = if args.verbose { "debug" } else { "warn" };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter)),
        )
        .with_writer(std::io::stderr)
        .init();

    match args.command {
        Some(Command::Serve {
            port,
            proxy,
            user_agent,
            stealth,
            workers,
        }) => {
            print_banner(port);
            if let Some(ref proxy) = proxy {
                tracing::info!("Using proxy: {}", proxy);
            }
            if let Some(ref ua) = user_agent {
                tracing::info!("User-Agent: {}", ua);
            }
            if stealth {
                tracing::info!("Stealth mode enabled (TLS fingerprint spoofing)");
            }

            if workers > 1 {
                tracing::info!("{} worker processes", workers);
                serve::run_multi_worker_serve(port, workers, proxy, stealth).await?;
            } else {
                obscura_cdp::start_with_options(port, proxy).await?;
            }
        }
        Some(Command::Fetch {
            url,
            dump,
            selector,
            wait,
            wait_until,
            user_agent,
            stealth,
            eval,
            quiet,
        }) => {
            fetch::run_fetch(
                &url,
                dump,
                selector,
                wait,
                &wait_until,
                user_agent,
                stealth,
                eval,
                quiet,
            )
            .await?;
        }
        Some(Command::Scrape {
            urls,
            eval,
            concurrency,
            format,
        }) => {
            scrape::run_parallel_scrape(urls, eval, concurrency, &format).await?;
        }
        None => {
            print_banner(args.port);
            if let Some(ref proxy) = args.proxy {
                tracing::info!("Using proxy: {}", proxy);
            }
            obscura_cdp::start_with_options(args.port, args.proxy).await?;
        }
    }

    Ok(())
}
