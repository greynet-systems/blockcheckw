use std::time::Instant;

use indicatif::{ProgressBar, ProgressStyle};
use tokio::task::JoinSet;
use tracing::info;

use crate::config::{CoreConfig, Protocol};
use crate::error::TaskResult;
use crate::firewall::nftables;
use crate::pipeline::worker_task::{execute_worker_task, WorkerTask};
use crate::worker::slot::WorkerSlot;

#[derive(Debug)]
pub struct StrategyResult {
    pub strategy_args: Vec<String>,
    pub result: TaskResult,
}

#[derive(Debug)]
pub struct RunStats {
    pub total: usize,
    pub completed: usize,
    pub successes: usize,
    pub failures: usize,
    pub errors: usize,
    pub elapsed: std::time::Duration,
}

impl RunStats {
    pub fn throughput(&self) -> f64 {
        if self.elapsed.as_secs_f64() > 0.0 {
            self.completed as f64 / self.elapsed.as_secs_f64()
        } else {
            0.0
        }
    }
}

/// Run strategies in parallel batches using worker slots.
pub async fn run_parallel(
    config: &CoreConfig,
    domain: &str,
    protocol: Protocol,
    strategies: &[Vec<String>],
    ips: &[String],
) -> (Vec<StrategyResult>, RunStats) {
    let start = Instant::now();
    let slots = WorkerSlot::create_slots(config.worker_count, config.base_qnum, config.base_local_port);

    // Prepare nftables table once
    if let Err(e) = nftables::prepare_table(&config.nft_table).await {
        let results: Vec<StrategyResult> = strategies
            .iter()
            .map(|args| StrategyResult {
                strategy_args: args.clone(),
                result: TaskResult::Error { error: e.clone() },
            })
            .collect();
        let stats = RunStats {
            total: strategies.len(),
            completed: strategies.len(),
            successes: 0,
            failures: 0,
            errors: strategies.len(),
            elapsed: start.elapsed(),
        };
        return (results, stats);
    }

    let pb = ProgressBar::new(strategies.len() as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} ({per_sec}, ETA {eta})"
        )
        .unwrap()
        .progress_chars("=>-"),
    );

    let mut all_results: Vec<StrategyResult> = Vec::with_capacity(strategies.len());
    let mut successes = 0usize;
    let mut failures = 0usize;
    let mut errors = 0usize;

    let batches: Vec<&[Vec<String>]> = strategies.chunks(config.worker_count).collect();

    for batch in batches {
        let mut join_set = JoinSet::new();

        for (index, strategy_args) in batch.iter().enumerate() {
            let slot = slots[index].clone();
            let config = config.clone();
            let task = WorkerTask {
                slot,
                domain: domain.to_string(),
                strategy_args: strategy_args.clone(),
                protocol,
                ips: ips.to_vec(),
            };

            join_set.spawn(async move {
                let result = execute_worker_task(&config, &task).await;
                (task.strategy_args, result)
            });
        }

        while let Some(join_result) = join_set.join_next().await {
            match join_result {
                Ok((strategy_args, task_result)) => {
                    match &task_result {
                        TaskResult::Success { .. } => successes += 1,
                        TaskResult::Failed { .. } => failures += 1,
                        TaskResult::Error { .. } => errors += 1,
                    }

                    let test_func = protocol.test_func_name();
                    pb.suspend(|| {
                        println!(
                            "- {test_func} ipv4 {domain} : nfqws2 {}",
                            strategy_args.join(" ")
                        );
                        println!("{task_result}");
                    });
                    pb.inc(1);

                    all_results.push(StrategyResult {
                        strategy_args,
                        result: task_result,
                    });
                }
                Err(join_err) => {
                    errors += 1;
                    pb.suspend(|| {
                        eprintln!("task join error: {join_err}");
                    });
                    pb.inc(1);
                }
            }
        }
    }

    pb.finish_and_clear();

    // Cleanup nftables table
    nftables::drop_table(&config.nft_table).await;

    let elapsed = start.elapsed();
    let stats = RunStats {
        total: strategies.len(),
        completed: all_results.len(),
        successes,
        failures,
        errors,
        elapsed,
    };

    info!(
        "Completed {}/{} strategies in {:.2}s ({:.1} strat/sec): {} success, {} failed, {} errors",
        stats.completed,
        stats.total,
        stats.elapsed.as_secs_f64(),
        stats.throughput(),
        stats.successes,
        stats.failures,
        stats.errors
    );

    (all_results, stats)
}
