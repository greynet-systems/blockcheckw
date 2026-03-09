use std::sync::Arc;

use clap::{Parser, Subcommand};
use indicatif::MultiProgress;
use tokio::signal;
use tracing::info;

use blockcheckw::config::{CoreConfig, Protocol};
use blockcheckw::error::TaskResult;
use blockcheckw::firewall::nftables;
use blockcheckw::network::dns;
use blockcheckw::pipeline::baseline;
use blockcheckw::pipeline::benchmark;
use blockcheckw::pipeline::runner::run_parallel;
use blockcheckw::strategy::generator;

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

    /// Scan domain for working DPI bypass strategies
    Scan {
        /// Target domain to check
        #[arg(short, long, default_value = "rutracker.org")]
        domain: String,

        /// Protocols to test (comma-separated: http,tls12,tls13)
        #[arg(short, long, default_value = "http,tls12,tls13")]
        protocols: String,
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
        Some(Command::Scan { domain, protocols }) => {
            tracing_subscriber::fmt()
                .with_env_filter(tracing_subscriber::EnvFilter::new("warn"))
                .init();

            let protocols = match blockcheckw::config::parse_protocols(&protocols) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("ERROR: {e}");
                    std::process::exit(1);
                }
            };
            run_scan(cli.workers, &domain, &protocols).await;
        }
        None => run_default(cli.workers).await,
    }
}

async fn run_scan(workers: usize, domain: &str, protocols: &[Protocol]) {
    let config = Arc::new(CoreConfig {
        worker_count: workers,
        ..CoreConfig::default()
    });

    // Signal handler: cleanup nftables on Ctrl+C
    let cleanup_config = config.clone();
    tokio::spawn(async move {
        if signal::ctrl_c().await.is_ok() {
            eprintln!("\nCtrl+C received, cleaning up...");
            nftables::drop_table(&cleanup_config.nft_table).await;
            std::process::exit(130);
        }
    });

    // 1. DNS resolve
    println!("=== DNS resolve ===");
    let ips = match dns::resolve_ipv4(domain).await {
        Ok(ips) => {
            println!("resolved {} -> {}", domain, ips.join(", "));
            ips
        }
        Err(e) => {
            eprintln!("ERROR: {e}");
            std::process::exit(1);
        }
    };

    // 2. Baseline per protocol
    println!("\n=== Baseline (without bypass) ===");
    let mut blocked_protocols = Vec::new();

    for &protocol in protocols {
        let result = baseline::test_baseline(domain, protocol, &config.curl_max_time).await;
        println!("  {}", baseline::format_baseline_verdict(&result));
        if result.is_blocked() {
            blocked_protocols.push(protocol);
        }
    }

    if blocked_protocols.is_empty() {
        println!("\nAll protocols are available without bypass. Nothing to scan.");
        return;
    }

    println!(
        "\nBlocked protocols: {}",
        blocked_protocols
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );

    // 3. Scan each blocked protocol
    let multi = MultiProgress::new();
    let mut summary: Vec<(Protocol, Vec<Vec<String>>, usize, usize, usize, f64)> = Vec::new();

    for &protocol in &blocked_protocols {
        println!("\n=== Scanning {} ===", protocol);
        let strategies = generator::generate_strategies(protocol);
        println!("  generated {} strategies, workers={}", strategies.len(), config.worker_count);

        let (results, stats) = run_parallel(
            &config,
            domain,
            protocol,
            &strategies,
            &ips,
            Some(&multi),
            None,
        )
        .await;

        let working: Vec<Vec<String>> = results
            .iter()
            .filter(|r| matches!(r.result, TaskResult::Success { .. }))
            .map(|r| r.strategy_args.clone())
            .collect();

        println!(
            "  completed: {} | success: {} | failed: {} | errors: {} | {:.1}s ({:.1} strat/sec)",
            stats.completed,
            stats.successes,
            stats.failures,
            stats.errors,
            stats.elapsed.as_secs_f64(),
            stats.throughput()
        );

        summary.push((
            protocol,
            working,
            stats.successes,
            stats.failures,
            stats.errors,
            stats.elapsed.as_secs_f64(),
        ));
    }

    // 4. Summary
    println!("\n=== Summary for {} ===", domain);

    // Available protocols (not blocked)
    for &protocol in protocols {
        if !blocked_protocols.contains(&protocol) {
            println!("  {}: working without bypass", protocol);
        }
    }

    // Blocked protocols results
    for (protocol, working, _successes, _failures, _errors, _elapsed) in &summary {
        if working.is_empty() {
            println!("  {}: no working strategies found", protocol);
        } else {
            println!("  {}: {} working strategies found", protocol, working.len());
            for args in working {
                println!("    nfqws2 {}", args.join(" "));
            }
        }
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

    let (results, stats) = run_parallel(&config, domain, protocol, &strategies, &ips, None, None).await;

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
