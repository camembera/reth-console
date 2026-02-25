use crate::engine::{EvalOutcome, evaluate_line};
use crate::output::print_value_for_chain;
use crate::rpc::RpcClient;
use eyre::Result;
use std::collections::BTreeMap;

pub async fn run_exec(
    rpc: &RpcClient,
    script: &str,
    aliases: &BTreeMap<String, String>,
    chain_id: Option<u64>,
) -> Result<()> {
    let mut last = None;
    match evaluate_line(rpc, aliases, script, &mut last).await? {
        EvalOutcome::Value(value) => print_value_for_chain(&value, chain_id),
        EvalOutcome::Help => print_help(),
        EvalOutcome::Noop | EvalOutcome::Exit => {}
    }
    Ok(())
}

fn print_help() {
    println!("Usage:");
    println!("  <method> [json_params]   (RPC call)");
    println!("  <alias>                  (e.g. eth.blockNumber)");
    println!("  .count | .len | .first | .last | .[0] | .[0].field | .map(.field)");
    println!("Examples:");
    println!("  eth.getBalance [\"0xabc...\", \"latest\"]");
    println!("  admin.peers");
    println!("In REPL, query commands apply to the last RPC result.");
    println!("That lets you do: admin.peers -> .count -> .[0]");
}
