use serde_json::Value;

pub fn print_value(value: &Value) {
    match value {
        Value::Array(items) => {
            println!("{} items", items.len());
            println!("{}", pretty(value));
        }
        _ => {
            println!("{}", pretty(value));
        }
    }

    let list_counts = collect_list_counts(value);
    if !list_counts.is_empty() {
        println!("-- list counts --");
        for count in list_counts {
            println!("{count}");
        }
    }

    let annotations = collect_annotations(value);
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

fn collect_annotations(value: &Value) -> Vec<String> {
    let mut out = Vec::new();
    walk("$", value, &mut out);
    out
}

fn collect_list_counts(value: &Value) -> Vec<String> {
    let mut out = Vec::new();
    walk_list_counts("$", value, &mut out);
    out
}

fn walk_list_counts(path: &str, value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                walk_list_counts(&format!("{path}.{k}"), v, out);
            }
        }
        Value::Array(items) => {
            if path != "$" {
                out.push(format!("{path}: {} items", items.len()));
            }
            for (idx, v) in items.iter().enumerate() {
                walk_list_counts(&format!("{path}[{idx}]"), v, out);
            }
        }
        _ => {}
    }
}

fn walk(path: &str, value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                walk(&format!("{path}.{k}"), v, out);
            }
        }
        Value::Array(items) => {
            for (idx, v) in items.iter().enumerate() {
                walk(&format!("{path}[{idx}]"), v, out);
            }
        }
        Value::String(s) => {
            if let Some(dec) = small_hex_to_dec(s) {
                out.push(format!("{path}: {s} -> {dec}"));
                if looks_like_wei(dec) {
                    out.push(format!("{path}: {dec} wei -> {} ETH", format_eth(dec)));
                }
            }
            if let Some(wei) = decimal_like_wei(s) {
                out.push(format!("{path}: {wei} wei -> {} ETH", format_eth(wei)));
            }
        }
        Value::Number(n) => {
            if let Some(wei) = n.as_u64().map(u128::from) {
                if looks_like_wei(wei) {
                    out.push(format!("{path}: {wei} wei -> {} ETH", format_eth(wei)));
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
        let notes = collect_annotations(&value);
        assert!(notes.iter().any(|n| n.contains("0x5208 -> 21000")));
        assert!(notes.iter().any(|n| n.contains("1 ETH")));
    }

    #[test]
    fn annotates_hex_wei_as_eth() {
        let value = serde_json::json!("0x0de0b6b3a7640000");
        let notes = collect_annotations(&value);
        assert!(notes.iter().any(|n| n.contains("1000000000000000000")));
        assert!(notes.iter().any(|n| n.contains("1 ETH")));
    }

    #[test]
    fn collects_nested_list_counts() {
        let value = serde_json::json!({
            "logs": [{"id": 1}, {"id": 2}],
            "txs": [1, 2, 3]
        });
        let counts = collect_list_counts(&value);
        assert!(counts.iter().any(|n| n == "$.logs: 2 items"));
        assert!(counts.iter().any(|n| n == "$.txs: 3 items"));
    }

    #[test]
    fn does_not_duplicate_top_level_array_count() {
        let value = serde_json::json!([1, 2, 3]);
        let counts = collect_list_counts(&value);
        assert!(counts.is_empty());
    }
}
