use crate::cli::RuntimeConfig;
use eyre::{Result, bail};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Http,
    Ws,
    Ipc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedEndpoint {
    pub raw: String,
    pub transport: Transport,
}

pub fn resolve_endpoint(cfg: &RuntimeConfig) -> Result<ResolvedEndpoint> {
    let raw = match &cfg.endpoint {
        Some(endpoint) => endpoint.clone(),
        None => cfg
            .datadir
            .join(&cfg.ipc_filename)
            .to_string_lossy()
            .to_string(),
    };

    let transport = detect_transport(&raw)?;
    Ok(ResolvedEndpoint { raw, transport })
}

fn detect_transport(endpoint: &str) -> Result<Transport> {
    if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        return Ok(Transport::Http);
    }
    if endpoint.starts_with("ws://") || endpoint.starts_with("wss://") {
        return Ok(Transport::Ws);
    }
    if endpoint.contains("://") {
        bail!("unsupported endpoint scheme in {endpoint:?}");
    }
    Ok(Transport::Ipc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::RuntimeConfig;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn cfg(endpoint: Option<&str>) -> RuntimeConfig {
        RuntimeConfig {
            endpoint: endpoint.map(ToOwned::to_owned),
            datadir: PathBuf::from("/tmp/reth"),
            ipc_filename: "reth.ipc".to_owned(),
            exec: None,
            http_headers: vec![],
            rpc_aliases: BTreeMap::new(),
            raw: false,
            yes: false,
        }
    }

    #[test]
    fn defaults_to_ipc_path() {
        let got = resolve_endpoint(&cfg(None)).unwrap();
        assert_eq!(got.transport, Transport::Ipc);
        assert!(got.raw.ends_with("reth.ipc"));
    }

    #[test]
    fn parses_http() {
        let got = resolve_endpoint(&cfg(Some("http://127.0.0.1:8545"))).unwrap();
        assert_eq!(got.transport, Transport::Http);
    }

    #[test]
    fn parses_https() {
        let got = resolve_endpoint(&cfg(Some("https://example.test"))).unwrap();
        assert_eq!(got.transport, Transport::Http);
    }

    #[test]
    fn parses_ws() {
        let got = resolve_endpoint(&cfg(Some("ws://127.0.0.1:8546"))).unwrap();
        assert_eq!(got.transport, Transport::Ws);
    }

    #[test]
    fn parses_wss() {
        let got = resolve_endpoint(&cfg(Some("wss://example.test/ws"))).unwrap();
        assert_eq!(got.transport, Transport::Ws);
    }

    #[test]
    fn rejects_unknown_scheme() {
        let err = resolve_endpoint(&cfg(Some("ftp://example.test"))).unwrap_err();
        assert!(err.to_string().contains("unsupported endpoint scheme"));
    }
}
