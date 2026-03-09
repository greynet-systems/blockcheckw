use crate::config::{CoreConfig, Protocol, NFQWS2_INIT_DELAY_MS};
use crate::error::{CurlVerdictAvailable, TaskResult};
use crate::firewall::nftables;
use crate::network::curl::{curl_test, interpret_curl_result, CurlVerdict};
use crate::worker::nfqws2::start_nfqws2;
use crate::worker::slot::WorkerSlot;

#[derive(Debug)]
pub struct WorkerTask {
    pub slot: WorkerSlot,
    pub domain: String,
    pub strategy_args: Vec<String>,
    pub protocol: Protocol,
    pub ips: Vec<String>,
}

/// Execute a full worker task cycle:
/// 1. Add nftables rule
/// 2. Start nfqws2
/// 3. Sleep for init delay
/// 4. Run curl test
/// 5. Interpret result
/// 6. Cleanup: kill nfqws2 + remove rule
pub async fn execute_worker_task(config: &CoreConfig, task: &WorkerTask) -> TaskResult {
    // Step 1: Add nftables rule
    let handle = match nftables::add_worker_rule(
        &config.nft_table,
        &task.slot.sport_range(),
        task.protocol.port(),
        task.slot.qnum,
        &task.ips,
    )
    .await
    {
        Ok(h) => h,
        Err(e) => {
            return TaskResult::Error { error: e };
        }
    };

    // Step 2: Start nfqws2
    let mut nfqws2_process = match start_nfqws2(config, task.slot.qnum, &task.strategy_args) {
        Ok(p) => p,
        Err(e) => {
            // Cleanup rule on nfqws2 start failure
            let _ = nftables::remove_rule(&config.nft_table, handle).await;
            return TaskResult::Error { error: e };
        }
    };

    // Step 3-6 in a block that guarantees cleanup
    let result = async {
        // Step 3: Wait for nfqws2 initialization
        tokio::time::sleep(std::time::Duration::from_millis(NFQWS2_INIT_DELAY_MS)).await;

        // Step 4: Run curl test
        let local_port = task.slot.local_port_arg();
        let curl_result = curl_test(
            task.protocol,
            &task.domain,
            Some(&local_port),
            &config.curl_max_time,
        )
        .await;

        // Step 5: Interpret result
        let verdict = interpret_curl_result(&curl_result, &task.domain);

        match verdict {
            CurlVerdict::Available => TaskResult::Success {
                verdict: CurlVerdictAvailable,
                strategy_args: task.strategy_args.clone(),
            },
            other => TaskResult::Failed { verdict: other },
        }
    }
    .await;

    // Step 6: Cleanup — always kill nfqws2 and remove rule
    nfqws2_process.kill().await;
    let _ = nftables::remove_rule(&config.nft_table, handle).await;

    result
}
