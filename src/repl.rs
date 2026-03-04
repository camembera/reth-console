use crate::endpoint::ResolvedEndpoint;
use crate::engine::{EvalOutcome, evaluate_line};
use crate::output::print_value_for_chain;
use crate::rpc::RpcClient;
use eyre::Result;
use rustyline::CompletionType;
use rustyline::config::Configurer;
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
use std::time::Instant;

pub async fn run_repl(
    rpc: &RpcClient,
    sentinel: Option<&RpcClient>,
    history_path: PathBuf,
    endpoint: ResolvedEndpoint,
    aliases: &BTreeMap<String, String>,
    chain_id: Option<u64>,
    has_bera_admin: bool,
    bera_admin_status: Option<Value>,
    sentinel_connected: bool,
    sentinel_connected_at: Instant,
) -> Result<()> {
    std::fs::create_dir_all(
        history_path
            .parent()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| PathBuf::from(".")),
    )?;

    let modules = rpc.supported_modules().await.unwrap_or_default();
    let helper = CompletionHelper::new(aliases, &modules, has_bera_admin, sentinel_connected);
    let mut editor: Editor<CompletionHelper, DefaultHistory> = Editor::new()?;
    editor.set_completion_type(CompletionType::List);
    editor.set_helper(Some(helper));
    if history_path.exists() {
        let _ = editor.load_history(&history_path);
    }

    println!("reth-console :: {}", endpoint.raw);
    print_startup_snapshot(rpc, chain_id, bera_admin_status.as_ref(), sentinel, sentinel_connected_at).await;
    println!("help: commands | ctrl-d/exit: quit");

    let mut last_rpc_result = None;
    loop {
        let line = editor.readline("reth> ");
        match line {
            Ok(line) => {
                if !line.trim().is_empty() {
                    let _ = editor.add_history_entry(line.as_str());
                }
                match evaluate_line(rpc, sentinel, aliases, &line, &mut last_rpc_result, has_bera_admin).await {
                    Ok(EvalOutcome::Noop) => {}
                    Ok(EvalOutcome::Exit) => break,
                    Ok(EvalOutcome::Help) => print_help(aliases, has_bera_admin, sentinel_connected),
                    Ok(EvalOutcome::Value(value)) => print_value_for_chain(&value, chain_id),
                    Ok(EvalOutcome::NeedsConfirmation {
                        method,
                        params,
                        warning,
                    }) => {
                        eprintln!("{}", warning);
                        match editor.readline("confirm [y/N]: ") {
                            Ok(resp) if resp.trim().eq_ignore_ascii_case("y") => {
                                match rpc.request_value(&method, params).await {
                                    Ok(value) => {
                                        print_value_for_chain(&value, chain_id);
                                    }
                                    Err(err) => eprintln!("error: {err}"),
                                }
                            }
                            _ => eprintln!("cancelled"),
                        }
                    }
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

async fn print_startup_snapshot(rpc: &RpcClient, chain_id: Option<u64>, bera_admin_status: Option<&Value>, sentinel: Option<&RpcClient>, sentinel_connected_at: Instant) {
    if let Some(status) = bera_admin_status {
        let client_version = status.get("client").and_then(|v| as_string(v));
        let network_id = status.get("networkId").and_then(|v| as_string(v));
        let head_number = status.get("head").and_then(|v| as_string(v));
        let peer_count_total = status.get("peerCountTotal").and_then(|v| hex_or_decimal_to_u64(v));
        let peer_count_inbound = status.get("peerCountInbound").and_then(|v| hex_or_decimal_to_u64(v));
        let peer_count_outbound = status.get("peerCountOutbound").and_then(|v| hex_or_decimal_to_u64(v));
        
        let peers_str = if let (Some(in_count), Some(out_count)) = (peer_count_inbound, peer_count_outbound) {
            format!("peers={} (in={} out={})", peer_count_total.unwrap_or(0), in_count, out_count)
        } else {
            format!("peers={}", peer_count_total.unwrap_or(0))
        };
        
        println!(
            "node :: {} | net={} 🐻⭐ | block={} | {}",
            client_version.unwrap_or_else(|| "unavailable".to_owned()),
            network_id.unwrap_or_else(|| "unavailable".to_owned()),
            head_number.unwrap_or_else(|| "unavailable".to_owned()),
            peers_str
        );
    } else {
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
            "node :: version={} | net={}{} | block={} | peers={}",
            version.unwrap_or_else(|| "unavailable".to_owned()),
            network.unwrap_or_else(|| "unavailable".to_owned()),
            chain_emoji(chain_id),
            block.unwrap_or_else(|| "unavailable".to_owned()),
            peers.unwrap_or_else(|| "unavailable".to_owned()),
        );
    }
    
    if let Some(sentinel_client) = sentinel {
        let uptime_secs = sentinel_connected_at.elapsed().as_secs();
        let uptime_str = format_uptime(uptime_secs);
        
        let node_count = sentinel_client
            .request_value("sentinel_config", None)
            .await
            .ok()
            .and_then(|v| {
                v.as_object()
                    .and_then(|obj| obj.get("nodes"))
                    .and_then(|nodes| nodes.as_array().map(|a| a.len()))
            })
            .unwrap_or(0);
        
        println!("sentinel :: up={} | nodes={}", uptime_str, node_count);
    }
}

fn format_uptime(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h", secs / 3600)
    }
}

fn chain_emoji(chain_id: Option<u64>) -> &'static str {
    match chain_id {
        Some(80_069) | Some(80_094) => " 🐻",
        _ => "",
    }
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

fn print_help(aliases: &BTreeMap<String, String>, has_bera_admin: bool, sentinel_connected: bool) {
    println!("Commands:");
    println!("  <method> [json_params]   (RPC call)");
    println!("  <alias>                  (e.g. eth.blockNumber)");
    println!("  TAB                      completion for aliases/methods");
    println!("  help | exit");
    println!("Queries (run against last RPC result):");
    println!("  .count | .len | .first | .last | .[0] | .[0].field | .map(.field)");
    println!("  examples:");
    println!("    admin.peers");
    println!("    .count");
    println!("    .[0]");
    println!("    .[0].caps");
    println!("    eth.getBalance [\"0xabc...\", \"latest\"]");
    if has_bera_admin {
        println!("beraAdmin (when detected):");
        println!("  peers                 detailed peer table");
        println!("  status                node identity and sync state");
        println!("  ban \"0xpeerId\"        ban peer (~12h)");
        println!("  penalize \"0xpeerId\" -100   penalize peer by value");
    }
    if sentinel_connected {
        println!("Sentinel (when connected):");
        println!("  scores                 peer threat scores from sentinel");
        println!("  subnets                banned subnets from sentinel");
        println!("  poll                   trigger sentinel poll");
        println!("  dryrun                 toggle sentinel dry-run mode");
    }
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
        has_bera_admin: bool,
        sentinel_connected: bool,
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
        if has_bera_admin {
            words.push("beraAdmin.".to_owned());
            words.push("beraAdmin_".to_owned());
            words.extend(default_module_dot_methods("beraAdmin"));
        }
        if sentinel_connected {
            words.push("sentinel.".to_owned());
            words.push("sentinel_".to_owned());
            words.extend(default_module_dot_methods("sentinel"));
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
        "beraAdmin" => vec![
            "beraAdmin.detailedPeers",
            "beraAdmin.nodeStatus",
            "beraAdmin.banPeer",
            "beraAdmin.penalizePeer",
        ],
        "sentinel" => vec![
            "sentinel.peerScores",
            "sentinel.bannedSubnets",
            "sentinel.triggerPoll",
            "sentinel.setDryRun",
            "sentinel.config",
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
        let helper = CompletionHelper::new(&aliases, &modules, false, false);
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
        let helper = CompletionHelper::new(&aliases, &modules, false, false);

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

    #[test]
    fn adds_bear_emoji_for_bera_chains() {
        assert_eq!(chain_emoji(Some(80_069)), " 🐻");
        assert_eq!(chain_emoji(Some(80_094)), " 🐻");
        assert_eq!(chain_emoji(Some(1)), "");
        assert_eq!(chain_emoji(None), "");
    }

    #[test]
    fn completion_includes_beraAdmin_methods_when_flag_provided() {
        let aliases = BTreeMap::new();
        let modules = BTreeMap::new();
        let helper = CompletionHelper::new(&aliases, &modules, true, false);
        assert!(helper.words.iter().any(|w| w == "beraAdmin.detailedPeers"));
        assert!(helper.words.iter().any(|w| w == "beraAdmin.nodeStatus"));
        assert!(helper.words.iter().any(|w| w == "beraAdmin.banPeer"));
        assert!(helper.words.iter().any(|w| w == "beraAdmin.penalizePeer"));
    }

    #[test]
    fn completion_excludes_beraAdmin_methods_when_flag_false() {
        let aliases = BTreeMap::new();
        let modules = BTreeMap::new();
        let helper = CompletionHelper::new(&aliases, &modules, false, false);
        assert!(!helper.words.iter().any(|w| w == "beraAdmin.detailedPeers"));
        assert!(!helper.words.iter().any(|w| w == "beraAdmin.nodeStatus"));
    }
}
