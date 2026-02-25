use serde_json::Value;

const DEFAULT_NATIVE_SYMBOL: &str = "ETH";

pub fn print_value_for_chain(value: &Value, chain_id: Option<u64>) {
    print_value_with_symbol(value, native_symbol_for_chain_id(chain_id));
}

pub fn native_symbol_for_chain_id(chain_id: Option<u64>) -> &'static str {
    match chain_id {
        Some(80_069) | Some(80_094) => "BERA",
        _ => DEFAULT_NATIVE_SYMBOL,
    }
}

fn print_value_with_symbol(value: &Value, native_symbol: &str) {
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
