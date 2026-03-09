use std::fmt;

pub const PORTS_PER_WORKER: u16 = 10;
pub const DESYNC_MARK: u32 = 0x10000000;
pub const NFQWS2_INIT_DELAY_MS: u64 = 50;

#[derive(Debug, Clone)]
pub struct CoreConfig {
    pub worker_count: usize,
    pub base_qnum: u16,
    pub base_local_port: u16,
    pub nft_table: String,
    pub nfqws2_path: String,
    pub curl_max_time: String,
    pub zapret_base: String,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            worker_count: 8,
            base_qnum: 200,
            base_local_port: 30000,
            nft_table: "zapret".to_string(),
            nfqws2_path: detect_nfqws2_path("/opt/zapret2"),
            curl_max_time: "1".to_string(),
            zapret_base: "/opt/zapret2".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Http,
    HttpsTls12,
    HttpsTls13,
}

impl Protocol {
    pub fn port(self) -> u16 {
        match self {
            Protocol::Http => 80,
            Protocol::HttpsTls12 | Protocol::HttpsTls13 => 443,
        }
    }

    pub fn test_func_name(self) -> &'static str {
        match self {
            Protocol::Http => "curl_test_http",
            Protocol::HttpsTls12 => "curl_test_https_tls12",
            Protocol::HttpsTls13 => "curl_test_https_tls13",
        }
    }
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Protocol::Http => write!(f, "HTTP"),
            Protocol::HttpsTls12 => write!(f, "HTTPS/TLS1.2"),
            Protocol::HttpsTls13 => write!(f, "HTTPS/TLS1.3"),
        }
    }
}

pub fn detect_nfqws2_path(zapret_base: &str) -> String {
    let arch = std::env::consts::ARCH;
    let binary_arch = match arch {
        "aarch64" | "arm" => "linux-arm64",
        _ => "linux-x86_64",
    };
    format!("{zapret_base}/binaries/{binary_arch}/nfqws2")
}
