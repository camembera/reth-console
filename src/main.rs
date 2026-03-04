mod cli;
mod command;
mod endpoint;
mod engine;
mod exec;
mod output;
mod query;
mod repl;
mod rpc;

use clap::Parser;
use cli::Cli;
use serde_json::Value;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let cli = Cli::parse();
    let mut cfg = cli.runtime_config()?;
    let endpoint = endpoint::resolve_endpoint(&cfg)?;
    let rpc = rpc::RpcClient::connect(&endpoint, &cfg.http_headers).await?;
    let chain_id = rpc
        .request_value("eth_chainId", None)
        .await
        .ok()
        .and_then(|v| parse_chain_id(&v));

    let bera_admin_status = rpc.request_value("beraAdmin_nodeStatus", None).await.ok();
    let has_bera_admin = bera_admin_status.is_some();

    if has_bera_admin {
        for (alias, method) in [
            ("peers", "beraAdmin_detailedPeers"),
            ("status", "beraAdmin_nodeStatus"),
            ("ban", "beraAdmin_banPeer"),
            ("penalize", "beraAdmin_penalizePeer"),
        ] {
            cfg.rpc_aliases.entry(alias.to_owned()).or_insert(method.to_owned());
        }
    }

    let sentinel = if let Some(ref sentinel_path) = cfg.sentinel {
        match rpc::RpcClient::connect(
            &endpoint::ResolvedEndpoint {
                raw: sentinel_path.clone(),
                transport: endpoint::Transport::Ipc,
            },
            &cfg.http_headers,
        )
        .await
        {
            Ok(client) => Some(client),
            Err(e) => {
                eprintln!("warning: sentinel not connected ({e})");
                None
            }
        }
    } else {
        None
    };

    if sentinel.is_some() {
        for (alias, method) in [
            ("scores", "sentinel_peerScores"),
            ("subnets", "sentinel_bannedSubnets"),
            ("poll", "sentinel_triggerPoll"),
            ("dryrun", "sentinel_setDryRun"),
        ] {
            cfg.rpc_aliases.entry(alias.to_owned()).or_insert(method.to_owned());
        }
    }

    if let Some(script) = cfg.exec {
        exec::run_exec(&rpc, sentinel.as_ref(), &script, &cfg.rpc_aliases, chain_id, has_bera_admin, cfg.yes).await?;
    } else {
        repl::run_repl(
            &rpc,
            sentinel.as_ref(),
            cfg.history_path(),
            endpoint,
            &cfg.rpc_aliases,
            chain_id,
            has_bera_admin,
            bera_admin_status,
        )
        .await?;
    }

    Ok(())
}

fn parse_chain_id(value: &Value) -> Option<u64> {
    match value {
        Value::String(s) => {
            if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                u64::from_str_radix(hex, 16).ok()
            } else {
                s.parse::<u64>().ok()
            }
        }
        Value::Number(n) => n.as_u64(),
        _ => None,
    }
}
