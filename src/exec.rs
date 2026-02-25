use crate::engine::{EvalOutcome, evaluate_line};
use crate::output::print_value;
use crate::rpc::RpcClient;
use eyre::Result;
use std::collections::BTreeMap;

pub async fn run_exec(
    rpc: &RpcClient,
    script: &str,
    aliases: &BTreeMap<String, String>,
) -> Result<()> {
    let mut last = None;
    match evaluate_line(rpc, aliases, script, &mut last).await? {
        EvalOutcome::Value(value) => print_value(&value),
        EvalOutcome::Help => print_help(),
        EvalOutcome::Noop | EvalOutcome::Exit => {}
    }
    Ok(())
}

fn print_help() {
    println!("Usage:");
    println!("  <method> [json_params]");
    println!("  <alias>                  (e.g. eth.blockNumber)");
    println!("  example: eth.getBalance [\"0xabc...\", \"latest\"]");
    println!("  example: eth.getBalance(\"0xabc...\", \"latest\")");
    println!("  .count | .len | .first | .last | .[0].field | .map(.field)");
}
