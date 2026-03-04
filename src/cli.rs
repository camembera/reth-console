use clap::Parser;
use eyre::{Result, eyre};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const DEFAULT_IPC_FILENAME: &str = "reth.ipc";

#[derive(Debug, Parser)]
#[command(name = "reth-console")]
#[command(about = "Standalone attach console for reth/bera-reth")]
pub struct Cli {
    /// Endpoint URL or IPC path. If omitted, defaults to datadir/<ipc-filename>.
    pub endpoint: Option<String>,

    /// Data directory used for default IPC endpoint and history file.
    #[arg(long)]
    pub datadir: Option<PathBuf>,

    /// IPC filename when endpoint is omitted.
    #[arg(long, default_value = DEFAULT_IPC_FILENAME)]
    pub ipc_filename: String,

    /// Optional script/command to run once and exit.
    #[arg(long = "exec")]
    pub exec: Option<String>,

    /// Additional HTTP headers in key:value format.
    #[arg(long = "http-header")]
    pub http_headers: Vec<String>,

    /// RPC alias in the form alias=rpc_method (repeatable).
    #[arg(long = "alias")]
    pub aliases: Vec<String>,

    /// Output raw JSON instead of formatted tables.
    #[arg(long)]
    pub raw: bool,

    /// Skip confirmation prompts for destructive actions (ban, penalize).
    #[arg(long)]
    pub yes: bool,

    /// Path to bera-sentinel IPC socket for sentinel commands.
    #[arg(long)]
    pub sentinel: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub endpoint: Option<String>,
    pub datadir: PathBuf,
    pub ipc_filename: String,
    pub exec: Option<String>,
    pub http_headers: Vec<(String, String)>,
    pub rpc_aliases: BTreeMap<String, String>,
    pub raw: bool,
    pub yes: bool,
    pub sentinel: Option<String>,
}

impl Cli {
    pub fn runtime_config(self) -> Result<RuntimeConfig> {
        let datadir = self.datadir.unwrap_or_else(default_datadir);
        let http_headers = parse_headers(&self.http_headers)?;
        let rpc_aliases = parse_aliases(&self.aliases)?;
        Ok(RuntimeConfig {
            endpoint: self.endpoint,
            datadir,
            ipc_filename: self.ipc_filename,
            exec: self.exec,
            http_headers,
            rpc_aliases,
            raw: self.raw,
            yes: self.yes,
            sentinel: self.sentinel,
        })
    }
}

impl RuntimeConfig {
    pub fn history_path(&self) -> PathBuf {
        self.datadir.join("reth-console-history")
    }
}

fn parse_headers(headers: &[String]) -> Result<Vec<(String, String)>> {
    headers
        .iter()
        .map(|h| {
            let (k, v) = h
                .split_once(':')
                .ok_or_else(|| eyre!("invalid --http-header value {h:?}, expected key:value"))?;
            if k.trim().is_empty() {
                return Err(eyre!("invalid --http-header value {h:?}, empty header key"));
            }
            Ok((k.trim().to_owned(), v.trim().to_owned()))
        })
        .collect()
}

fn parse_aliases(aliases: &[String]) -> Result<BTreeMap<String, String>> {
    let mut out = default_aliases();
    for alias in aliases {
        let (name, method) = alias
            .split_once('=')
            .ok_or_else(|| eyre!("invalid --alias value {alias:?}, expected alias=rpc_method"))?;
        if name.trim().is_empty() || method.trim().is_empty() {
            return Err(eyre!("invalid --alias value {alias:?}, empty side"));
        }
        out.insert(name.trim().to_owned(), method.trim().to_owned());
    }
    Ok(out)
}

fn default_aliases() -> BTreeMap<String, String> {
    [
        ("eth.blockNumber", "eth_blockNumber"),
        ("net.version", "net_version"),
        ("web3.clientVersion", "web3_clientVersion"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect()
}

fn default_datadir() -> PathBuf {
    if cfg!(target_os = "macos") {
        if let Some(home) = dirs::home_dir() {
            return home
                .join("Library")
                .join("Application Support")
                .join("reth");
        }
    }
    dirs::data_dir()
        .unwrap_or_else(|| Path::new(".").to_path_buf())
        .join("reth")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_header() {
        let got = parse_headers(&["Authorization: Bearer token".to_owned()]).unwrap();
        assert_eq!(
            got,
            vec![("Authorization".to_owned(), "Bearer token".to_owned())]
        );
    }

    #[test]
    fn merges_aliases() {
        let got = parse_aliases(&["bn=eth_blockNumber".to_owned()]).unwrap();
        assert_eq!(got.get("bn"), Some(&"eth_blockNumber".to_owned()));
        assert!(got.contains_key("eth.blockNumber"));
    }

    #[test]
    fn rejects_invalid_header_shape() {
        let err = parse_headers(&["Authorization bearer token".to_owned()]).unwrap_err();
        assert!(err.to_string().contains("expected key:value"));
    }

    #[test]
    fn rejects_empty_header_key() {
        let err = parse_headers(&[": token".to_owned()]).unwrap_err();
        assert!(err.to_string().contains("empty header key"));
    }

    #[test]
    fn rejects_invalid_alias_shape() {
        let err = parse_aliases(&["eth.blockNumber".to_owned()]).unwrap_err();
        assert!(err.to_string().contains("expected alias=rpc_method"));
    }

    #[test]
    fn rejects_empty_alias_side() {
        let err = parse_aliases(&["bn=".to_owned()]).unwrap_err();
        assert!(err.to_string().contains("empty side"));
    }

    #[test]
    fn raw_flag_parsed() {
        let cli = Cli::try_parse_from(["reth-console", "--raw"]).unwrap();
        assert!(cli.raw);
    }

    #[test]
    fn raw_flag_default_false() {
        let cli = Cli::try_parse_from(["reth-console"]).unwrap();
        assert!(!cli.raw);
    }

    #[test]
    fn yes_flag_parsed() {
        let cli = Cli::try_parse_from(["reth-console", "--yes"]).unwrap();
        assert!(cli.yes);
    }

    #[test]
    fn yes_flag_default_false() {
        let cli = Cli::try_parse_from(["reth-console"]).unwrap();
        assert!(!cli.yes);
    }

    #[test]
    fn runtime_config_includes_raw_and_yes() {
        let cli = Cli::try_parse_from(["reth-console", "--raw", "--yes"]).unwrap();
        let cfg = cli.runtime_config().unwrap();
        assert!(cfg.raw);
        assert!(cfg.yes);
    }

    #[test]
    fn sentinel_flag_parsed() {
        let cli = Cli::try_parse_from(["reth-console", "--sentinel", "/tmp/sentinel.sock"]).unwrap();
        assert_eq!(cli.sentinel, Some("/tmp/sentinel.sock".to_string()));
    }

    #[test]
    fn sentinel_flag_optional() {
        let cli = Cli::try_parse_from(["reth-console"]).unwrap();
        assert!(cli.sentinel.is_none());
    }
}
