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
}

pub async fn evaluate_line(
    rpc: &RpcClient,
    aliases: &BTreeMap<String, String>,
    line: &str,
    last_result: &mut Option<Value>,
) -> Result<EvalOutcome> {
    match parse_input(line)? {
        InputCommand::Empty => Ok(EvalOutcome::Noop),
        InputCommand::Exit => Ok(EvalOutcome::Exit),
        InputCommand::Help => Ok(EvalOutcome::Help),
        InputCommand::Query(expr) => {
            let value = last_result
                .as_ref()
                .ok_or_else(|| eyre!("no last result available for query"))?;
            let next = apply_query(&expr, value)?;
            *last_result = Some(next.clone());
            Ok(EvalOutcome::Value(next))
        }
        InputCommand::Rpc { method, params } => {
            let normalized_method = normalize_rpc_method(&method);
            let value = rpc.request_value(&normalized_method, params).await?;
            *last_result = Some(value.clone());
            Ok(EvalOutcome::Value(value))
        }
        InputCommand::Alias(alias) => {
            let method = resolve_alias_method(aliases, &alias);
            let value = rpc.request_value(&method, None).await?;
            *last_result = Some(value.clone());
            Ok(EvalOutcome::Value(value))
        }
    }
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
        let err = match parse_input(".count").unwrap() {
            InputCommand::Query(expr) => apply_query(&expr, &Value::Null).unwrap_err(),
            _ => unreachable!(),
        };
        assert!(err.to_string().contains("only apply to arrays and objects"));
    }

    #[test]
    fn query_updates_last_result_value() {
        let mut last = Some(json!([{ "n": 1 }, { "n": 2 }]));
        let next = apply_query(".[1].n", last.as_ref().unwrap()).unwrap();
        last = Some(next.clone());
        assert_eq!(next, json!(2));
        assert_eq!(last, Some(json!(2)));
    }
}
