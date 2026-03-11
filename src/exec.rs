use crate::engine::{EvalOutcome, evaluate_line};
use crate::output::print_value_for_chain_raw;
use crate::rpc::RpcClient;
use eyre::Result;
use std::collections::BTreeMap;

pub async fn run_exec(
    rpc: &RpcClient,
    sentinel: Option<&RpcClient>,
    script: &str,
    aliases: &BTreeMap<String, String>,
    chain_id: Option<u64>,
    raw: bool,
    has_bera_admin: bool,
    _yes: bool,
) -> Result<()> {
    let mut last = None;
    match evaluate_line(rpc, sentinel, aliases, script, &mut last, has_bera_admin).await? {
        EvalOutcome::Value(value) => print_value_for_chain_raw(&value, chain_id, raw),
        EvalOutcome::Help => print_help(),
        EvalOutcome::Noop | EvalOutcome::Exit => {}
        EvalOutcome::NeedsConfirmation {
            method: _,
            params: _,
            warning,
        } => {
            eprintln!("{}", warning);
            std::process::exit(1);
        }
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
