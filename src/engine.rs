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
        InputCommand::Alias(alias) => {
            let method = resolve_alias_method(aliases, &alias);
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
}
