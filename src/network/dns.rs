use std::collections::HashSet;

use crate::error::BlockcheckError;
use crate::system::process::run_process;

const DNS_TIMEOUT_MS: u64 = 10_000;

fn is_ipv4(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    parts.iter().all(|p| !p.is_empty() && p.parse::<u8>().is_ok())
}

async fn resolve_with_getent(domain: &str) -> Option<Vec<String>> {
    let args = vec!["getent", "ahostsv4", domain];
    let result = run_process(&args, DNS_TIMEOUT_MS).await.ok()?;
    if result.exit_code != 0 {
        return None;
    }

    let ips: HashSet<String> = result
        .stdout
        .lines()
        .filter_map(|line| {
            let token = line.split_whitespace().next()?;
            if is_ipv4(token) {
                Some(token.to_string())
            } else {
                None
            }
        })
        .collect();

    Some(ips.into_iter().collect())
}

async fn resolve_with_nslookup(domain: &str) -> Option<Vec<String>> {
    let args = vec!["nslookup", domain];
    let result = run_process(&args, DNS_TIMEOUT_MS).await.ok()?;
    if result.exit_code != 0 {
        return None;
    }

    let answer_section = result.stdout.split_once("Name:")?;
    let ips: HashSet<String> = answer_section
        .1
        .split_whitespace()
        .filter(|token| is_ipv4(token))
        .map(|s| s.to_string())
        .collect();

    Some(ips.into_iter().collect())
}

pub async fn resolve_ipv4(domain: &str) -> Result<Vec<String>, BlockcheckError> {
    let ips = if let Some(ips) = resolve_with_getent(domain).await {
        Some(ips)
    } else {
        resolve_with_nslookup(domain).await
    };

    match ips {
        Some(ips) if !ips.is_empty() => Ok(ips),
        Some(_) => Err(BlockcheckError::DnsNoAddresses {
            domain: domain.to_string(),
        }),
        None => Err(BlockcheckError::DnsResolveFailed {
            domain: domain.to_string(),
            reason: "getent and nslookup both failed".to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_ipv4_valid() {
        assert!(is_ipv4("192.168.1.1"));
        assert!(is_ipv4("0.0.0.0"));
        assert!(is_ipv4("255.255.255.255"));
        assert!(is_ipv4("1.2.3.4"));
        assert!(is_ipv4("172.67.182.217"));
    }

    #[test]
    fn test_is_ipv4_invalid() {
        assert!(!is_ipv4("256.1.1.1"));
        assert!(!is_ipv4("1.2.3"));
        assert!(!is_ipv4("1.2.3.4.5"));
        assert!(!is_ipv4(""));
        assert!(!is_ipv4("abc.def.ghi.jkl"));
        assert!(!is_ipv4("192.168.1"));
        assert!(!is_ipv4("1.2.3."));
        assert!(!is_ipv4(".1.2.3"));
        assert!(!is_ipv4("rutracker.org"));
        assert!(!is_ipv4("STREAM"));
    }
}
