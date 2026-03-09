use crate::error::BlockcheckError;
use crate::system::process::run_process;

const CHAIN_NAME: &str = "postnat";
const NFT_TIMEOUT_MS: u64 = 5_000;

#[derive(Debug, Clone, Copy)]
pub struct RuleHandle(pub u32);

async fn run_nft(args: &[&str]) -> Result<String, BlockcheckError> {
    let mut cmd: Vec<&str> = vec!["nft"];
    cmd.extend_from_slice(args);

    let result = run_process(&cmd, NFT_TIMEOUT_MS).await?;

    if result.exit_code == 0 {
        Ok(result.stdout)
    } else {
        Err(BlockcheckError::Nftables {
            command: cmd.join(" "),
            stderr: result.stderr,
        })
    }
}

/// Create the nftables table and postrouting chain.
pub async fn prepare_table(table: &str) -> Result<(), BlockcheckError> {
    run_nft(&["add", "table", "inet", table]).await?;
    run_nft(&[
        "add", "chain", "inet", table, CHAIN_NAME,
        "{ type filter hook postrouting priority 102; }",
    ])
    .await?;
    Ok(())
}

/// Add a per-worker nftables rule and return its handle for later removal.
pub async fn add_worker_rule(
    table: &str,
    sport_range: &str,
    dport: u16,
    qnum: u16,
    ips: &[String],
) -> Result<RuleHandle, BlockcheckError> {
    let ip_set = ips.join(", ");
    let dport_str = dport.to_string();
    let qnum_str = qnum.to_string();
    let ip_expr = format!("{{ {ip_set} }}");

    let args: Vec<&str> = vec![
        "--echo", "--handle",
        "add", "rule", "inet", table, CHAIN_NAME,
        "meta", "nfproto", "ipv4",
        "tcp", "sport", sport_range,
        "tcp", "dport", &dport_str,
        "mark", "and", "0x10000000", "==", "0",
        "ip", "daddr", &ip_expr,
        "ct", "mark", "set", "ct", "mark", "or", "0x10000000",
        "queue", "num", &qnum_str,
    ];

    let stdout = run_nft(&args).await?;

    parse_handle(&stdout)
}

/// Remove a specific rule by its handle.
pub async fn remove_rule(table: &str, handle: RuleHandle) -> Result<(), BlockcheckError> {
    let handle_str = handle.0.to_string();
    run_nft(&["delete", "rule", "inet", table, CHAIN_NAME, "handle", &handle_str])
        .await?;
    Ok(())
}

/// Drop the entire nftables table. Ignores errors (cleanup).
pub async fn drop_table(table: &str) {
    let _ = run_nft(&["delete", "table", "inet", table]).await;
}

fn parse_handle(stdout: &str) -> Result<RuleHandle, BlockcheckError> {
    // nft --echo --handle outputs lines like: "# handle 42"
    let re_pattern = "# handle ";
    for line in stdout.lines() {
        if let Some(pos) = line.find(re_pattern) {
            let num_str = &line[pos + re_pattern.len()..];
            if let Ok(n) = num_str.trim().parse::<u32>() {
                return Ok(RuleHandle(n));
            }
        }
    }
    Err(BlockcheckError::NftHandleParse {
        output: stdout.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_handle() {
        let output = "add rule inet zapret postnat meta nfproto ipv4 tcp sport 30000-30009 tcp dport 80 mark and 0x10000000 == 0 ip daddr { 1.2.3.4 } ct mark set ct mark or 0x10000000 queue num 200 # handle 42\n";
        let handle = parse_handle(output).unwrap();
        assert_eq!(handle.0, 42);
    }

    #[test]
    fn test_parse_handle_missing() {
        let output = "some other output\n";
        assert!(parse_handle(output).is_err());
    }

    #[test]
    fn test_parse_handle_multiline() {
        let output = "table inet zapret {\n}\nadd rule ... # handle 137\n";
        let handle = parse_handle(output).unwrap();
        assert_eq!(handle.0, 137);
    }
}
