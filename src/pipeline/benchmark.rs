use crate::config::{CoreConfig, Protocol};
use crate::pipeline::runner::run_parallel;

#[derive(Debug, Clone)]
pub struct BenchmarkPoint {
    pub workers: usize,
    pub elapsed_secs: f64,
    pub throughput: f64,
    pub errors: usize,
}

#[derive(Debug)]
pub struct BenchmarkResult {
    pub points: Vec<BenchmarkPoint>,
    pub recommended_workers: usize,
    pub strategy_count: usize,
    pub domain: String,
    pub protocol: Protocol,
}

pub fn generate_strategies(count: usize) -> Vec<Vec<String>> {
    (1..=count)
        .map(|ttl| {
            vec![
                "--dpi-desync=fake".to_string(),
                format!("--dpi-desync-ttl={ttl}"),
            ]
        })
        .collect()
}

pub fn worker_counts_to_test(max: usize) -> Vec<usize> {
    let mut counts: Vec<usize> = (0..)
        .map(|p| 1usize << p)
        .take_while(|&n| n <= max)
        .collect();
    // Ensure max is included even if not a power of 2
    if counts.last() != Some(&max) {
        counts.push(max);
    }
    counts
}

pub fn default_max_workers() -> usize {
    let cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    cpus * 16
}

/// Find optimal worker count using 90%-of-max-throughput threshold.
///
/// 1. Filter out points with errors
/// 2. Find max throughput
/// 3. Pick the minimum worker count that reaches 90% of max throughput
pub fn find_optimal(points: &[BenchmarkPoint]) -> usize {
    let clean: Vec<&BenchmarkPoint> = points.iter().filter(|p| p.errors == 0).collect();

    if clean.is_empty() {
        return points
            .first()
            .map(|p| p.workers)
            .unwrap_or(1);
    }

    let max_throughput = clean
        .iter()
        .map(|p| p.throughput)
        .fold(0.0_f64, f64::max);

    let threshold = max_throughput * 0.90;

    clean
        .iter()
        .filter(|p| p.throughput >= threshold)
        .min_by_key(|p| p.workers)
        .map(|p| p.workers)
        .unwrap_or_else(|| clean.last().unwrap().workers)
}

pub async fn run_benchmark(
    strategy_count: usize,
    max_workers: usize,
    raw: bool,
) -> BenchmarkResult {
    use indicatif::{ProgressBar, ProgressStyle};

    let domain = "rutracker.org";
    let protocol = Protocol::Http;
    let ips = vec!["172.67.182.217".to_string()];
    let strategies = generate_strategies(strategy_count);
    let worker_counts = worker_counts_to_test(max_workers);

    if !raw {
        println!("=== blockcheckw benchmark ===");
        println!(
            "domain={domain}  protocol={protocol}  strategies={strategy_count}  max_workers={max_workers}"
        );
        println!();
    }

    // Print table header before creating progress bar
    println!(
        "{:>8}  {:>10}  {:>10}  {:>7}  {:>6}",
        "Workers", "Elapsed(s)", "Throughput", "Speedup", "Errors"
    );
    println!(
        "{:>8}  {:>10}  {:>10}  {:>7}  {:>6}",
        "-------", "----------", "----------", "-------", "------"
    );

    let total_steps = worker_counts.len() * strategy_count;
    let pb = if raw {
        ProgressBar::hidden()
    } else {
        let pb = ProgressBar::new(total_steps as u64);
        pb.set_style(
            ProgressStyle::with_template(
                "{spinner:.green} [{elapsed_precise}] [{bar:20.cyan/blue}] {pos}/{len} ({msg}, ETA {eta})"
            )
            .unwrap()
            .progress_chars("=>-"),
        );
        pb.set_message("-- strat/s");
        pb.enable_steady_tick(std::time::Duration::from_millis(100));
        pb
    };

    let mut points = Vec::new();
    let mut base_throughput: Option<f64> = None;
    let mut total_strategies_done: usize = 0;
    let bench_start = std::time::Instant::now();

    for &wc in &worker_counts {
        let config = CoreConfig {
            worker_count: wc,
            ..CoreConfig::default()
        };

        pb.set_message(format!("w={wc} ..."));

        let (_, stats) = run_parallel(&config, domain, protocol, &strategies, &ips).await;

        let point = BenchmarkPoint {
            workers: wc,
            elapsed_secs: stats.elapsed.as_secs_f64(),
            throughput: stats.throughput(),
            errors: stats.errors,
        };

        if base_throughput.is_none() {
            base_throughput = Some(point.throughput);
        }
        let base = base_throughput.unwrap_or(1.0);
        let speedup = if base > 0.0 { point.throughput / base } else { 0.0 };

        total_strategies_done += strategy_count;
        let overall_rate = total_strategies_done as f64 / bench_start.elapsed().as_secs_f64();

        pb.suspend(|| {
            println!(
                "{:>8}  {:>10.2}  {:>8.1}/s  {:>6.1}x  {:>6}",
                point.workers, point.elapsed_secs, point.throughput, speedup, point.errors
            );
        });
        pb.inc(strategy_count as u64);
        pb.set_message(format!("{overall_rate:.1} strat/s"));

        points.push(point);

        // Small delay between runs for cleanup
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }

    pb.finish_and_clear();

    let recommended_workers = find_optimal(&points);

    if !raw {
        println!();
        println!("Recommended: blockcheckw -w {recommended_workers}");
    }

    BenchmarkResult {
        points,
        recommended_workers,
        strategy_count,
        domain: domain.to_string(),
        protocol,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_optimal_basic() {
        let points = vec![
            BenchmarkPoint { workers: 1, elapsed_secs: 72.1, throughput: 0.9, errors: 0 },
            BenchmarkPoint { workers: 2, elapsed_secs: 36.8, throughput: 1.7, errors: 0 },
            BenchmarkPoint { workers: 4, elapsed_secs: 19.5, throughput: 3.3, errors: 0 },
            BenchmarkPoint { workers: 8, elapsed_secs: 10.4, throughput: 6.2, errors: 0 },
            BenchmarkPoint { workers: 16, elapsed_secs: 6.1, throughput: 10.5, errors: 0 },
            BenchmarkPoint { workers: 32, elapsed_secs: 3.5, throughput: 18.3, errors: 0 },
            BenchmarkPoint { workers: 64, elapsed_secs: 2.4, throughput: 27.1, errors: 0 },
            BenchmarkPoint { workers: 128, elapsed_secs: 2.8, throughput: 22.7, errors: 0 },
        ];
        // 90% of 27.1 = 24.39 → only 64 (27.1) passes
        assert_eq!(find_optimal(&points), 64);
    }

    #[test]
    fn test_find_optimal_skips_errors() {
        let points = vec![
            BenchmarkPoint { workers: 1, elapsed_secs: 10.0, throughput: 1.0, errors: 0 },
            BenchmarkPoint { workers: 4, elapsed_secs: 3.0, throughput: 3.5, errors: 0 },
            BenchmarkPoint { workers: 8, elapsed_secs: 1.5, throughput: 7.0, errors: 5 },
        ];
        // worker=8 has errors → excluded; max clean throughput = 3.5; 90% = 3.15 → 4 passes
        assert_eq!(find_optimal(&points), 4);
    }

    #[test]
    fn test_find_optimal_all_errors() {
        let points = vec![
            BenchmarkPoint { workers: 4, elapsed_secs: 5.0, throughput: 2.0, errors: 1 },
            BenchmarkPoint { workers: 8, elapsed_secs: 3.0, throughput: 3.0, errors: 2 },
        ];
        // All have errors → fallback to first point's workers
        assert_eq!(find_optimal(&points), 4);
    }

    #[test]
    fn test_worker_counts_to_test() {
        assert_eq!(worker_counts_to_test(64), vec![1, 2, 4, 8, 16, 32, 64]);
        assert_eq!(worker_counts_to_test(48), vec![1, 2, 4, 8, 16, 32, 48]);
        assert_eq!(worker_counts_to_test(1), vec![1]);
    }

    #[test]
    fn test_generate_strategies() {
        let strats = generate_strategies(3);
        assert_eq!(strats.len(), 3);
        assert_eq!(strats[0], vec!["--dpi-desync=fake", "--dpi-desync-ttl=1"]);
        assert_eq!(strats[2], vec!["--dpi-desync=fake", "--dpi-desync-ttl=3"]);
    }
}
