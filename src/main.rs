use std::sync::Arc;

use tokio::signal;
use tracing::info;

use blockcheckw::config::{CoreConfig, Protocol};
use blockcheckw::firewall::nftables;
use blockcheckw::pipeline::runner::run_parallel;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = Arc::new(CoreConfig::default());

    // Signal handler: cleanup nftables on Ctrl+C
    let cleanup_config = config.clone();
    tokio::spawn(async move {
        if signal::ctrl_c().await.is_ok() {
            info!("Ctrl+C received, cleaning up nftables table...");
            nftables::drop_table(&cleanup_config.nft_table).await;
            std::process::exit(130);
        }
    });

    // Example: test a few strategies against a domain
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
