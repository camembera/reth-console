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
    let cfg = cli.runtime_config()?;
    let endpoint = endpoint::resolve_endpoint(&cfg)?;
    let rpc = rpc::RpcClient::connect(&endpoint, &cfg.http_headers).await?;
    let chain_id = rpc
        .request_value("eth_chainId", None)
        .await
        .ok()
        .and_then(|v| parse_chain_id(&v));

    if let Some(script) = cfg.exec {
        exec::run_exec(&rpc, &script, &cfg.rpc_aliases, chain_id).await?;
    } else {
        repl::run_repl(
            &rpc,
            cfg.history_path(),
            endpoint,
            &cfg.rpc_aliases,
            chain_id,
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
