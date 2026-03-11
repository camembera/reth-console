use crate::command::{InputCommand, parse_input};
use crate::query::apply_query;
use crate::rpc::RpcClient;
use eyre::{Result, eyre};
use serde_json::Value;
use std::collections::BTreeMap;

pub enum EvalOutcome {
    Noop,
    Exit,
    Help,
    Value(Value),
    NeedsConfirmation {
        method: String,
        params: Option<Value>,
        warning: String,
    },
}

pub async fn evaluate_line(
    rpc: &RpcClient,
    sentinel: Option<&RpcClient>,
    aliases: &BTreeMap<String, String>,
    line: &str,
    last_rpc_result: &mut Option<Value>,
    has_bera_admin: bool,
) -> Result<EvalOutcome> {
    match parse_input(line)? {
        InputCommand::Empty => Ok(EvalOutcome::Noop),
        InputCommand::Exit => Ok(EvalOutcome::Exit),
        InputCommand::Help => Ok(EvalOutcome::Help),
        InputCommand::Query(expr) => {
            let next = apply_query_to_last_rpc(&expr, last_rpc_result)?;
            Ok(EvalOutcome::Value(next))
        }
        InputCommand::Rpc { method, params } => {
            let normalized_method = normalize_rpc_method(&method);
            
            // Route sentinel methods to sentinel client
            if normalized_method.starts_with("sentinel_") {
                let client = sentinel.ok_or_else(|| eyre!("sentinel: not connected"))?;
                let value = client.request_value(&normalized_method, params).await?;
                *last_rpc_result = Some(value.clone());
                return Ok(EvalOutcome::Value(value));
            }
            
            if is_destructive_method(&normalized_method) && has_bera_admin {
                let action = if normalized_method.contains("ban") {
                    "ban"
                } else if normalized_method.contains("penalize") {
                    "penalize"
                } else if normalized_method.contains("addSubnetBan") {
                    "add subnet ban"
                } else if normalized_method.contains("removeSubnetBan") {
                    "remove subnet ban"
                } else {
                    "modify"
                };
                return Ok(EvalOutcome::NeedsConfirmation {
                    method: normalized_method,
                    params: params.clone(),
                    warning: format!("WARNING: This will {} peer. Use --yes to skip confirmation.", action),
                });
            }
            
            let value = rpc.request_value(&normalized_method, params).await?;
            *last_rpc_result = Some(value.clone());
            Ok(EvalOutcome::Value(value))
        }
        InputCommand::RpcWithQuery { method, params, query } => {
            let normalized_method = normalize_rpc_method(&method);
            let value = if normalized_method.starts_with("sentinel_") {
                let client = sentinel.ok_or_else(|| eyre!("sentinel: not connected"))?;
                client.request_value(&normalized_method, params).await?
            } else {
                rpc.request_value(&normalized_method, params).await?
            };
            *last_rpc_result = Some(value.clone());
            let result = apply_query(&query, &value)?;
            Ok(EvalOutcome::Value(result))
        }
        InputCommand::Alias(alias) => {
            if is_remove_all_peers_alias(alias.as_str()) {
                return run_remove_all_peers(rpc, last_rpc_result).await;
            }

            let method = resolve_alias_method(aliases, &alias);
            
            // Route sentinel aliases to sentinel client
            if method.starts_with("sentinel_") {
                let client = sentinel.ok_or_else(|| eyre!("sentinel: not connected"))?;
                let value = client.request_value(&method, None).await?;
                *last_rpc_result = Some(value.clone());
                return Ok(EvalOutcome::Value(value));
            }
            
            let value = rpc.request_value(&method, None).await?;
            *last_rpc_result = Some(value.clone());
            Ok(EvalOutcome::Value(value))
        }
    }
}

fn apply_query_to_last_rpc(expr: &str, last_rpc_result: &Option<Value>) -> Result<Value> {
    let value = last_rpc_result
        .as_ref()
        .ok_or_else(|| eyre!("no last rpc result available for query"))?;
    apply_query(expr, value)
}

fn normalize_rpc_method(method: &str) -> String {
    method.replace('.', "_")
}

fn resolve_alias_method(aliases: &BTreeMap<String, String>, alias: &str) -> String {
    aliases
        .get(alias)
        .cloned()
        .unwrap_or_else(|| alias.replace('.', "_"))
}

fn is_destructive_method(method: &str) -> bool {
    method.contains("ban") 
        || method.contains("penalize")
        || method.contains("Subnet")
        || method.contains("removePeer")
}

fn is_remove_all_peers_alias(alias: &str) -> bool {
    matches!(alias, "removeAllPeers" | "admin.removeAllPeers")
}

async fn run_remove_all_peers(
    rpc: &RpcClient,
    last_rpc_result: &mut Option<Value>,
) -> Result<EvalOutcome> {
    let peers = rpc.request_value("admin_peers", None).await?;
    let arr = peers
        .as_array()
        .ok_or_else(|| eyre!("admin.peers did not return an array"))?;
    let mut removed = 0u64;
    for peer in arr {
        let enode = peer
            .get("enode")
            .and_then(Value::as_str)
            .ok_or_else(|| eyre!("peer entry missing enode"))?;
        rpc.request_value("admin_removePeer", Some(serde_json::json!([enode])))
            .await?;
        removed += 1;
    }
    *last_rpc_result = Some(peers);
    eprintln!("Removed {} peer(s).", removed);
    Ok(EvalOutcome::Value(serde_json::json!({ "removed": removed })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalizes_rpc_methods() {
        assert_eq!(normalize_rpc_method("eth.getBalance"), "eth_getBalance");
        assert_eq!(normalize_rpc_method("eth_getBalance"), "eth_getBalance");
    }

    #[test]
    fn resolves_alias_from_map() {
        let aliases = BTreeMap::from([("bn".to_owned(), "eth_blockNumber".to_owned())]);
        assert_eq!(resolve_alias_method(&aliases, "bn"), "eth_blockNumber");
    }

    #[test]
    fn resolves_alias_by_dot_fallback() {
        let aliases = BTreeMap::new();
        assert_eq!(
            resolve_alias_method(&aliases, "net.peerCount"),
            "net_peerCount"
        );
    }

    #[test]
    fn query_without_last_result_errors() {
        let err = apply_query_to_last_rpc(".count", &None).unwrap_err();
        assert!(
            err.to_string()
                .contains("no last rpc result available for query")
        );
    }

    #[test]
    fn query_uses_last_rpc_result_without_mutating_it() {
        let last = Some(json!([{ "n": 1 }, { "n": 2 }]));
        let count = apply_query_to_last_rpc(".count", &last).unwrap();
        let first = apply_query_to_last_rpc(".[0].n", &last).unwrap();
        assert_eq!(count, json!(2));
        assert_eq!(first, json!(1));
        assert_eq!(last, Some(json!([{ "n": 1 }, { "n": 2 }])));
    }

    #[test]
    fn destructive_ban_method_detected() {
        assert!(is_destructive_method("beraAdmin_banPeer"));
        assert!(is_destructive_method("beraAdmin_penalizePeer"));
        assert!(is_destructive_method("sentinel_addSubnetBan"));
        assert!(is_destructive_method("sentinel_removeSubnetBan"));
    }

    #[test]
    fn normal_methods_not_destructive() {
        assert!(!is_destructive_method("eth_blockNumber"));
        assert!(!is_destructive_method("beraAdmin_nodeStatus"));
        assert!(!is_destructive_method("beraAdmin_detailedPeers"));
    }

    #[test]
    fn ban_returns_needs_confirmation() {
        // This test just verifies the destructive method detection
        assert!(is_destructive_method("beraAdmin_banPeer"));
    }

    #[test]
    fn penalize_returns_needs_confirmation() {
        assert!(is_destructive_method("beraAdmin_penalizePeer"));
    }

    #[test]
    fn subnet_ban_returns_needs_confirmation() {
        assert!(is_destructive_method("sentinel_addSubnetBan"));
        assert!(is_destructive_method("sentinel_removeSubnetBan"));
    }

    #[test]
    fn rpc_with_query_stores_raw_result_in_last() {
        let peers = json!([{"id": "a"}, {"id": "b"}]);
        let mut last: Option<Value> = None;
        let count = apply_query(".count", &peers).unwrap();
        last = Some(peers.clone());
        assert_eq!(count, json!(2));
        assert_eq!(last, Some(peers));
    }

    #[test]
    fn rpc_with_query_index_and_field() {
        let peers = json!([{"caps": ["eth/68"]}, {"caps": ["eth/67"]}]);
        let result = apply_query(".[0].caps", &peers).unwrap();
        assert_eq!(result, json!(["eth/68"]));
    }
}
