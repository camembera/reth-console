use crate::endpoint::ResolvedEndpoint;
use crate::engine::{EvalOutcome, evaluate_line};
use crate::output::print_value;
use crate::rpc::RpcClient;
use eyre::Result;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{Context, Editor, Helper};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub async fn run_repl(
    rpc: &RpcClient,
    history_path: PathBuf,
    endpoint: ResolvedEndpoint,
    aliases: &BTreeMap<String, String>,
) -> Result<()> {
    std::fs::create_dir_all(
        history_path
            .parent()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| PathBuf::from(".")),
    )?;

    let modules = rpc.supported_modules().await.unwrap_or_default();
    let helper = CompletionHelper::new(aliases, &modules);
    let mut editor: Editor<CompletionHelper, DefaultHistory> = Editor::new()?;
    editor.set_helper(Some(helper));
    if history_path.exists() {
        let _ = editor.load_history(&history_path);
    }

    println!("reth-console :: {}", endpoint.raw);
    print_startup_snapshot(rpc).await;
    println!("help: commands | ctrl-d/exit: quit");

    let mut last_result = None;
    loop {
        let line = editor.readline("reth> ");
        match line {
            Ok(line) => {
                if !line.trim().is_empty() {
                    let _ = editor.add_history_entry(line.as_str());
                }
                match evaluate_line(rpc, aliases, &line, &mut last_result).await {
                    Ok(EvalOutcome::Noop) => {}
                    Ok(EvalOutcome::Exit) => break,
                    Ok(EvalOutcome::Help) => print_help(aliases),
                    Ok(EvalOutcome::Value(value)) => print_value(&value),
                    Err(err) => eprintln!("error: {err}"),
                }
            }
            Err(ReadlineError::Interrupted) => {}
            Err(ReadlineError::Eof) => break,
            Err(err) => return Err(err.into()),
        }
    }

    let _ = editor.save_history(&history_path);
    Ok(())
}

async fn print_startup_snapshot(rpc: &RpcClient) {
    let version = rpc
        .request_value("web3_clientVersion", None)
        .await
        .ok()
        .and_then(|v| as_string(&v));
    let block = rpc
        .request_value("eth_blockNumber", None)
        .await
        .ok()
        .and_then(|v| hex_or_decimal_to_u64(&v).map(|n| n.to_string()));
    let peers = rpc
        .request_value("net_peerCount", None)
        .await
        .ok()
        .and_then(|v| hex_or_decimal_to_u64(&v).map(|n| n.to_string()));
    let network = rpc
        .request_value("net_version", None)
        .await
        .ok()
        .and_then(|v| as_string(&v));

    println!(
        "node :: version={} | net={} | block={} | peers={}",
        version.unwrap_or_else(|| "unavailable".to_owned()),
        network.unwrap_or_else(|| "unavailable".to_owned()),
        block.unwrap_or_else(|| "unavailable".to_owned()),
        peers.unwrap_or_else(|| "unavailable".to_owned()),
    );
}

fn as_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn hex_or_decimal_to_u64(value: &Value) -> Option<u64> {
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

fn print_help(aliases: &BTreeMap<String, String>) {
    println!("Commands:");
    println!("  <method> [json_params]");
    println!("  <alias>                  (e.g. eth.blockNumber)");
    println!("  examples:");
    println!("    eth.blockNumber");
    println!("    eth.getBalance [\"0xabc...\", \"latest\"]");
    println!("    eth.getBalance(\"0xabc...\", \"latest\")");
    println!("  .count | .len | .first | .last | .[0].field | .map(.field)");
    println!("  help | exit");
    if !aliases.is_empty() {
        println!("Aliases:");
        for (alias, method) in aliases {
            println!("  {alias} -> {method}");
        }
    }
}

struct CompletionHelper {
    words: Vec<String>,
}

impl CompletionHelper {
    fn new(
        aliases: &BTreeMap<String, String>,
        modules: &BTreeMap<String, String>,
    ) -> CompletionHelper {
        let mut words = vec![
            "help".to_owned(),
            "exit".to_owned(),
            "quit".to_owned(),
            ".count".to_owned(),
            ".len".to_owned(),
            ".first".to_owned(),
            ".last".to_owned(),
            ".map(".to_owned(),
        ];
        words.extend(aliases.keys().cloned());
        for method in aliases.values() {
            words.push(method.clone());
            if let Some(dot) = rpc_method_to_dot(method) {
                words.push(dot);
            }
        }
        for module in modules.keys() {
            words.push(format!("{module}."));
            words.push(format!("{module}_"));
            words.extend(default_module_dot_methods(module));
        }
        words.sort();
        words.dedup();
        CompletionHelper { words }
    }
}

fn rpc_method_to_dot(method: &str) -> Option<String> {
    let (module, rest) = method.split_once('_')?;
    if module.is_empty() || rest.is_empty() {
        return None;
    }
    Some(format!("{module}.{rest}"))
}

fn default_module_dot_methods(module: &str) -> Vec<String> {
    match module {
        "eth" => vec![
            "eth.blockNumber",
            "eth.getBalance",
            "eth.getBlockByHash",
            "eth.getBlockByNumber",
            "eth.getBlockReceipts",
            "eth.getBlockTransactionCountByHash",
            "eth.getBlockTransactionCountByNumber",
            "eth.getCode",
            "eth.getLogs",
            "eth.getStorageAt",
            "eth.getTransactionByHash",
            "eth.getTransactionCount",
            "eth.getTransactionReceipt",
            "eth.gasPrice",
            "eth.maxPriorityFeePerGas",
        ],
        "net" => vec!["net.version", "net.peerCount", "net.listening"],
        "web3" => vec!["web3.clientVersion", "web3.sha3"],
        "txpool" => vec!["txpool.content", "txpool.status", "txpool.inspect"],
        "admin" => vec![
            "admin.nodeInfo",
            "admin.peers",
            "admin.addPeer",
            "admin.removePeer",
        ],
        "debug" => vec![
            "debug.traceBlockByHash",
            "debug.traceBlockByNumber",
            "debug.traceTransaction",
        ],
        _ => vec![],
    }
    .into_iter()
    .map(ToOwned::to_owned)
    .collect()
}

impl Helper for CompletionHelper {}
impl Validator for CompletionHelper {}
impl Highlighter for CompletionHelper {}
impl Hinter for CompletionHelper {
    type Hint = String;
}

impl Completer for CompletionHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let safe_pos = pos.min(line.len());
        let up_to_cursor = &line[..safe_pos];
        let start = up_to_cursor
            .rfind(char::is_whitespace)
            .map(|i| i + 1)
            .unwrap_or(0);
        let needle = &up_to_cursor[start..];
        let matches = self
            .words
            .iter()
            .filter(|word| word.starts_with(needle))
            .map(|word| Pair {
                display: word.clone(),
                replacement: word.clone(),
            })
            .collect();
        Ok((start, matches))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustyline::Context;
    use rustyline::completion::Completer;
    use rustyline::history::DefaultHistory;
    use serde_json::json;

    #[test]
    fn includes_eth_get_completion_words() {
        let aliases = BTreeMap::new();
        let modules = BTreeMap::from([("eth".to_owned(), "1.0".to_owned())]);
        let helper = CompletionHelper::new(&aliases, &modules);
        assert!(helper.words.iter().any(|w| w == "eth.getBlockByNumber"));
        assert!(helper.words.iter().any(|w| w == "eth.getBlockByHash"));
    }

    #[test]
    fn parses_hex_or_decimal_numbers() {
        assert_eq!(
            hex_or_decimal_to_u64(&Value::String("0x10".to_owned())),
            Some(16)
        );
        assert_eq!(
            hex_or_decimal_to_u64(&Value::String("42".to_owned())),
            Some(42)
        );
        assert_eq!(
            hex_or_decimal_to_u64(&Value::Number(42u64.into())),
            Some(42)
        );
    }

    #[test]
    fn parses_uppercase_hex_and_rejects_invalid_numbers() {
        assert_eq!(
            hex_or_decimal_to_u64(&Value::String("0X10".to_owned())),
            Some(16)
        );
        assert_eq!(
            hex_or_decimal_to_u64(&Value::String("0xzz".to_owned())),
            None
        );
        assert_eq!(
            hex_or_decimal_to_u64(&Value::String("not-a-number".to_owned())),
            None
        );
    }

    #[test]
    fn converts_string_or_number_values() {
        assert_eq!(
            as_string(&json!("reth/1.2.3")),
            Some("reth/1.2.3".to_owned())
        );
        assert_eq!(as_string(&json!(42)), Some("42".to_owned()));
        assert_eq!(as_string(&json!(true)), None);
    }

    #[test]
    fn rpc_method_to_dot_handles_edge_cases() {
        assert_eq!(
            rpc_method_to_dot("eth_getBalance"),
            Some("eth.getBalance".to_owned())
        );
        assert_eq!(rpc_method_to_dot("eth"), None);
        assert_eq!(rpc_method_to_dot("_getBalance"), None);
        assert_eq!(rpc_method_to_dot("eth_"), None);
    }

    #[test]
    fn unknown_module_has_no_default_methods() {
        assert!(default_module_dot_methods("unknown").is_empty());
    }

    #[test]
    fn completion_matches_prefix_and_respects_word_start() {
        let aliases = BTreeMap::from([("bn".to_owned(), "eth_blockNumber".to_owned())]);
        let modules = BTreeMap::from([("eth".to_owned(), "1.0".to_owned())]);
        let helper = CompletionHelper::new(&aliases, &modules);

        let history = DefaultHistory::new();
        let ctx = Context::new(&history);
        let (start, hits) = helper.complete("eth.getB", "eth.getB".len(), &ctx).unwrap();
        assert_eq!(start, 0);
        assert!(hits.iter().any(|p| p.replacement == "eth.getBalance"));

        let (start2, hits2) = helper
            .complete("call eth.getB", "call eth.getB".len(), &ctx)
            .unwrap();
        assert_eq!(start2, 5);
        assert!(hits2.iter().any(|p| p.replacement == "eth.getBalance"));

        let (_start3, hits3) = helper.complete("zzz", 3, &ctx).unwrap();
        assert!(hits3.is_empty());
    }
}
