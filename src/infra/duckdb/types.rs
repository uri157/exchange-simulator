use serde_json::Value;

use crate::error::AppError;

/// Extrae un i64 de `row[idx]` asumiendo que viene como JSON number.
pub fn get_i64(row: &Vec<Value>, idx: usize, field: &str) -> Result<i64, AppError> {
    row.get(idx)
        .and_then(|v| v.as_i64())
        .ok_or_else(|| AppError::External(format!("missing or invalid {field} (i64)")))
}

/// Extrae un f64 de `row[idx]` asumiendo que viene como string numérica (formato Binance).
pub fn get_f64_from_str(row: &Vec<Value>, idx: usize, field: &str) -> Result<f64, AppError> {
    row.get(idx)
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::External(format!("missing {field} (str)")))?
        .parse::<f64>()
        .map_err(|e| AppError::External(format!("parse {field}: {e}")))
}

/// Inferencia simple de base/quote a partir del símbolo (heurística común).
pub fn infer_base_quote(symbol: &str) -> (String, String) {
    const COMMON_QUOTES: [&str; 6] = ["USDT", "USD", "BUSD", "FDUSD", "BTC", "ETH"];
    for quote in COMMON_QUOTES.iter() {
        if let Some(base) = symbol.strip_suffix(quote) {
            if !base.is_empty() {
                return (base.to_string(), (*quote).to_string());
            }
        }
    }
    let split = symbol.len().saturating_div(2);
    (symbol[..split].to_string(), symbol[split..].to_string())
}
