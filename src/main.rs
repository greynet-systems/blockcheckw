use std::sync::Arc;

use clap::{Parser, Subcommand};
use tokio::signal;
use tracing::info;

use blockcheckw::config::{CoreConfig, Protocol};
use blockcheckw::firewall::nftables;
use blockcheckw::pipeline::benchmark;
use blockcheckw::pipeline::runner::run_parallel;

#[derive(Parser)]
#[command(name = "blockcheckw", about = "Parallel DPI bypass strategy scanner")]
struct Cli {
    /// Number of parallel workers
    #[arg(short, long, default_value_t = 8)]
    workers: usize,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run parallel scaling benchmark to find optimal worker count
    Benchmark {
        /// Number of strategies to generate (fake TTL 1..N)
        #[arg(short, long, default_value_t = 64)]
        strategies: usize,

        /// Maximum number of workers to test (default: CPU cores * 16)
        #[arg(short = 'M', long)]
        max_workers: Option<usize>,

        /// Raw output: table only, no recommendation (for scripts)
        #[arg(long)]
        raw: bool,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Benchmark {
            strategies,
            max_workers,
            raw,
        }) => {
            tracing_subscriber::fmt()
                .with_env_filter(tracing_subscriber::EnvFilter::new("warn"))
                .init();

            let max = max_workers.unwrap_or_else(benchmark::default_max_workers);
            benchmark::run_benchmark(strategies, max, raw).await;
        }
        None => run_default(cli.workers).await,
    }
}

async fn run_default(workers: usize) {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = Arc::new(CoreConfig {
        worker_count: workers,
        ..CoreConfig::default()
    });

    // Signal handler: cleanup nftables on Ctrl+C
    let cleanup_config = config.clone();
    tokio::spawn(async move {
        if signal::ctrl_c().await.is_ok() {
            info!("Ctrl+C received, cleaning up nftables table...");
            nftables::drop_table(&cleanup_config.nft_table).await;
            std::process::exit(130);
        }
    });

    let domain = "rutracker.org";
    let protocol = Protocol::Http;
    let ips = vec!["172.67.182.217".to_string()];

    let strategies: Vec<Vec<String>> = vec![
        vec!["--dpi-desync=fake".to_string(), "--dpi-desync-ttl=1".to_string()],
        vec!["--dpi-desync=fake".to_string(), "--dpi-desync-ttl=2".to_string()],
        vec!["--dpi-desync=fake".to_string(), "--dpi-desync-ttl=3".to_string()],
    ];

    info!("blockcheckw starting: {protocol} {domain}");
    info!("workers={}, strategies={}", config.worker_count, strategies.len());

    let (results, stats) = run_parallel(&config, domain, protocol, &strategies, &ips).await;

    info!("=== Results ===");
    for r in &results {
        info!("nfqws2 {} : {}", r.strategy_args.join(" "), r.result);
    }

    info!(
        "Total: {} | Success: {} | Failed: {} | Errors: {} | {:.2}s ({:.1} strat/sec)",
        stats.total,
        stats.successes,
        stats.failures,
        stats.errors,
        stats.elapsed.as_secs_f64(),
        stats.throughput()
    );
}
