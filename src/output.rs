use serde_json::Value;

const DEFAULT_NATIVE_SYMBOL: &str = "ETH";

pub fn print_value_for_chain(value: &Value, chain_id: Option<u64>) {
    print_value_with_symbol(value, native_symbol_for_chain_id(chain_id), false);
}

pub fn print_value_for_chain_raw(value: &Value, chain_id: Option<u64>, raw: bool) {
    if raw {
        println!("{}", pretty(value));
    } else {
        print_value_for_chain(value, chain_id);
    }
}

pub fn native_symbol_for_chain_id(chain_id: Option<u64>) -> &'static str {
    match chain_id {
        Some(80_069) | Some(80_094) => "BERA",
        _ => DEFAULT_NATIVE_SYMBOL,
    }
}

fn print_value_with_symbol(value: &Value, native_symbol: &str, raw: bool) {
    if raw {
        println!("{}", pretty(value));
        return;
    }
    
    if let Some(table) = try_format_detailed_peers(value) {
        println!("{}", table);
        return;
    }
    if let Some(status) = try_format_node_status(value) {
        println!("{}", status);
        return;
    }

    println!("{}", pretty(value));

    let annotations = collect_annotations_with_symbol(value, native_symbol);
    if !annotations.is_empty() {
        println!("-- interpreted values --");
        for note in annotations {
            println!("{note}");
        }
    }
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

fn collect_annotations_with_symbol(value: &Value, native_symbol: &str) -> Vec<String> {
    let mut out = Vec::new();
    walk("$", value, native_symbol, &mut out);
    out
}

fn walk(path: &str, value: &Value, native_symbol: &str, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                walk(&format!("{path}.{k}"), v, native_symbol, out);
            }
        }
        Value::Array(items) => {
            for (idx, v) in items.iter().enumerate() {
                walk(&format!("{path}[{idx}]"), v, native_symbol, out);
            }
        }
        Value::String(s) => {
            if let Some(dec) = small_hex_to_dec(s) {
                out.push(format!("{path}: {s} -> {dec}"));
                if looks_like_wei(dec) {
                    out.push(format!(
                        "{path}: {dec} wei -> {} {native_symbol}",
                        format_eth(dec)
                    ));
                }
            }
            if let Some(wei) = decimal_like_wei(s) {
                out.push(format!(
                    "{path}: {wei} wei -> {} {native_symbol}",
                    format_eth(wei)
                ));
            }
        }
        Value::Number(n) => {
            if let Some(wei) = n.as_u64().map(u128::from) {
                if looks_like_wei(wei) {
                    out.push(format!(
                        "{path}: {wei} wei -> {} {native_symbol}",
                        format_eth(wei)
                    ));
                }
            }
        }
        _ => {}
    }
}

fn small_hex_to_dec(input: &str) -> Option<u128> {
    if !(input.starts_with("0x") || input.starts_with("0X")) {
        return None;
    }
    let hex = &input[2..];
    if hex.is_empty() || hex.len() > 32 {
        return None;
    }
    u128::from_str_radix(hex, 16).ok()
}

fn decimal_like_wei(input: &str) -> Option<u128> {
    if input.is_empty() || !input.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let value = input.parse::<u128>().ok()?;
    if looks_like_wei(value) {
        Some(value)
    } else {
        None
    }
}

fn looks_like_wei(value: u128) -> bool {
    // Heuristic: large integer range typically used for wei amounts.
    (1_000_000_000_000_000u128..=1_000_000_000_000_000_000_000_000_000_000_000u128).contains(&value)
}

fn format_eth(wei: u128) -> String {
    const WEI_PER_ETH: u128 = 1_000_000_000_000_000_000;
    let whole = wei / WEI_PER_ETH;
    let frac = wei % WEI_PER_ETH;
    if frac == 0 {
        return whole.to_string();
    }
    let mut frac_str = format!("{frac:018}");
    while frac_str.ends_with('0') {
        frac_str.pop();
    }
    format!("{whole}.{frac_str}")
}

fn try_format_detailed_peers(value: &Value) -> Option<String> {
    let peers = value.as_array()?;
    if peers.is_empty() {
        return None;
    }
    
    let first = peers.first()?;
    if !first.is_object() {
        return None;
    }
    
    let obj = first.as_object()?;
    if !obj.contains_key("peerId") || !obj.contains_key("remoteAddr") {
        return None;
    }
    
    let mut lines = vec![];
    let header = format!(
        "{:<20} {:<20} {:<4} {:<4} {:<6} {:<16} {:<8} {:<10}",
        "PEER", "ADDR", "DIR", "REP", "BLOCK", "CLIENT", "STATE", "PoG"
    );
    lines.push(header);
    
    for peer in peers {
        if let Some(peer_obj) = peer.as_object() {
            let peer_id = peer_obj
                .get("peerId")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let addr = peer_obj
                .get("remoteAddr")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let direction = peer_obj
                .get("direction")
                .and_then(|v| v.as_str())
                .map(|s| &s[..s.len().min(3)])
                .unwrap_or("-");
            let reputation = peer_obj
                .get("reputation")
                .and_then(|v| v.as_u64())
                .map(|r| r.to_string())
                .unwrap_or_else(|| "?".to_string());
            let block = peer_obj
                .get("latestBlock")
                .and_then(|v| v.as_u64())
                .map(|b| b.to_string())
                .unwrap_or_else(|| "?".to_string());
            let client = peer_obj
                .get("clientVersion")
                .and_then(|v| v.as_str())
                .unwrap_or("-");
            let client_short = if client.len() > 16 {
                format!("{}..","&client[..13]")
            } else {
                client.to_string()
            };
            let state = peer_obj
                .get("connectionState")
                .and_then(|v| v.as_str())
                .unwrap_or("-");
            
            let pog_str = if let Some(pog) = peer_obj.get("pog") {
                if pog.is_null() {
                    "-".to_string()
                } else if let Some(pog_obj) = pog.as_object() {
                    let failures = pog_obj
                        .get("failureCount")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let confirmations = pog_obj
                        .get("lastResult")
                        .and_then(|v| v.as_str())
                        .map(|_| 1)
                        .unwrap_or(0);
                    format!("{}/{}", failures, confirmations)
                } else {
                    "-".to_string()
                }
            } else {
                "-".to_string()
            };
            
            let peer_short = if peer_id.len() > 12 {
                format!("{}..{}", &peer_id[..8], &peer_id[peer_id.len()-4..])
            } else {
                peer_id.to_string()
            };
            
            let line = format!(
                "{:<20} {:<20} {:<4} {:<4} {:<6} {:<16} {:<8} {:<10}",
                peer_short, addr, direction, reputation, block, client_short, state, pog_str
            );
            lines.push(line);
        }
    }
    
    Some(lines.join("\n"))
}

fn try_format_node_status(value: &Value) -> Option<String> {
    let obj = value.as_object()?;
    
    if !obj.contains_key("chainId") && !obj.contains_key("genesisHash") && !obj.contains_key("headNumber") {
        return None;
    }
    
    let chain = obj
        .get("chainId")
        .or_else(|| obj.get("networkId"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let genesis = obj
        .get("genesisHash")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let genesis_short = if genesis.len() > 12 {
        format!("{}..{}", &genesis[..6], &genesis[genesis.len()-4..])
    } else {
        genesis.to_string()
    };
    let head = obj
        .get("headNumber")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let head_hash = obj
        .get("head")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let head_hash_short = if head_hash.len() > 12 {
        format!("{}..{}", &head_hash[..6], &head_hash[head_hash.len()-4..])
    } else {
        head_hash.to_string()
    };
    let syncing = obj
        .get("syncing")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let peers_total = obj
        .get("peerCountTotal")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let peers_in = obj
        .get("peerCountInbound")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let peers_out = obj
        .get("peerCountOutbound")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let client = obj
        .get("clientVersion")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    
    let output = format!(
        "chain={}  genesis={}  fork=unknown\nhead={} ({})  syncing={}\npeers={} (in={} out={})  client={}  net={}",
        chain, genesis_short, head, head_hash_short, syncing, peers_total, peers_in, peers_out, client, chain
    );
    
    Some(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_small_hex() {
        assert_eq!(small_hex_to_dec("0x2a"), Some(42));
        assert_eq!(
            small_hex_to_dec("0xffffffffffffffff"),
            Some(u64::MAX as u128)
        );
        assert_eq!(
            small_hex_to_dec("0x10000000000000000"),
            Some(18446744073709551616)
        );
        assert_eq!(
            small_hex_to_dec("0x100000000000000000000000000000000"),
            None
        );
    }

    #[test]
    fn formats_eth() {
        assert_eq!(format_eth(1_000_000_000_000_000_000), "1");
        assert_eq!(format_eth(1_500_000_000_000_000_000), "1.5");
    }

    #[test]
    fn annotates_wei_and_hex() {
        let value = serde_json::json!({
            "gasUsed": "0x5208",
            "amount": "1000000000000000000"
        });
        let notes = collect_annotations_with_symbol(&value, DEFAULT_NATIVE_SYMBOL);
        assert!(notes.iter().any(|n| n.contains("0x5208 -> 21000")));
        assert!(notes.iter().any(|n| n.contains("1 ETH")));
    }

    #[test]
    fn annotates_hex_wei_as_eth() {
        let value = serde_json::json!("0x0de0b6b3a7640000");
        let notes = collect_annotations_with_symbol(&value, DEFAULT_NATIVE_SYMBOL);
        assert!(notes.iter().any(|n| n.contains("1000000000000000000")));
        assert!(notes.iter().any(|n| n.contains("1 ETH")));
    }

    #[test]
    fn detects_bera_native_symbol() {
        assert_eq!(native_symbol_for_chain_id(Some(80_069)), "BERA");
        assert_eq!(native_symbol_for_chain_id(Some(80_094)), "BERA");
        assert_eq!(native_symbol_for_chain_id(Some(1)), "ETH");
        assert_eq!(native_symbol_for_chain_id(None), "ETH");
    }

    #[test]
    fn annotates_native_symbol_for_chain() {
        let value = serde_json::json!("1000000000000000000");
        let notes = collect_annotations_with_symbol(&value, "BERA");
        assert!(notes.iter().any(|n| n.contains("1 BERA")));
    }
}
