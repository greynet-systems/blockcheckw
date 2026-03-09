use std::sync::Arc;

use clap::{Parser, Subcommand};
use tokio::signal;
use tracing::info;

use blockcheckw::config::{CoreConfig, Protocol};
use blockcheckw::firewall::nftables;
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
    /// Run parallel scaling benchmark
    Benchmark {
        /// Number of strategies to generate (fake TTL 1..N)
        #[arg(short, long, default_value_t = 64)]
        strategies: usize,

        /// Run scaling test: iterate powers of 2 up to --workers
        #[arg(long)]
        scaling: bool,
    },
}

fn generate_strategies(count: usize) -> Vec<Vec<String>> {
    (1..=count)
        .map(|ttl| {
            vec![
                "--dpi-desync=fake".to_string(),
                format!("--dpi-desync-ttl={ttl}"),
            ]
        })
        .collect()
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Benchmark { strategies, scaling }) => {
            run_benchmark(cli.workers, strategies, scaling).await
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

async fn run_benchmark(workers: usize, strategy_count: usize, scaling: bool) {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("warn"))
        .init();

    let domain = "rutracker.org";
    let protocol = Protocol::Http;
    let ips = vec!["172.67.182.217".to_string()];
    let strategies = generate_strategies(strategy_count);

    let worker_counts: Vec<usize> = if scaling {
        // Powers of 2 up to the requested worker count
        (0..)
            .map(|p| 1usize << p)
            .take_while(|&n| n <= workers)
            .chain(std::iter::once(workers))
            .collect::<Vec<_>>()
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    } else {
        vec![workers]
    };

    println!("=== blockcheckw parallel scaling benchmark ===");
    println!("domain={domain}  protocol={protocol}  strategies={strategy_count}");
    println!();
    println!(
        "{:<10} {:<12} {:<14} {:<10} {:<10} {:<10}",
        "Workers", "Elapsed(s)", "Throughput", "Success", "Failed", "Errors"
    );
    println!("{}", "-".repeat(66));

    for &wc in &worker_counts {
        let config = CoreConfig {
            worker_count: wc,
            ..CoreConfig::default()
        };

        let (_, stats) = run_parallel(&config, domain, protocol, &strategies, &ips).await;

        println!(
            "{:<10} {:<12.2} {:<14.1} {:<10} {:<10} {:<10}",
            wc,
            stats.elapsed.as_secs_f64(),
            stats.throughput(),
            stats.successes,
            stats.failures,
            stats.errors
        );

        // Small delay between runs for cleanup
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }

    println!("{}", "=".repeat(66));
}
