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

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let cli = Cli::parse();
    let cfg = cli.runtime_config()?;
    let endpoint = endpoint::resolve_endpoint(&cfg)?;
    let rpc = rpc::RpcClient::connect(&endpoint, &cfg.http_headers).await?;

    if let Some(script) = cfg.exec {
        exec::run_exec(&rpc, &script, &cfg.rpc_aliases).await?;
    } else {
        repl::run_repl(&rpc, cfg.history_path(), endpoint, &cfg.rpc_aliases).await?;
    }

    Ok(())
}
