use std::{fmt, time::Duration};

use rand::{thread_rng, Rng};
use reqwest::{Client, StatusCode};
use serde::{de, Deserialize, Deserializer};
use tokio::time::sleep;
use tracing::warn;

use crate::{error::AppError, infra::config::AppConfig};

#[derive(Clone, Debug)]
pub struct BinanceClient {
    pub base_url: String,
    pub http: Client,
    pub retry_max: u32,
    pub retry_base_ms: u64,
}

impl BinanceClient {
    pub fn new_from_config(cfg: &AppConfig) -> Self {
        let timeout = Duration::from_millis(cfg.binance_http_timeout_ms.max(1));
        let http = Client::builder()
            .user_agent("exchange-simulator/binance-client")
            .timeout(timeout)
            .build()
            .expect("reqwest client for binance");

        Self {
            base_url: cfg.binance_base_url.trim_end_matches('/').to_string(),
            http,
            retry_max: cfg.binance_retry_max,
            retry_base_ms: cfg.binance_retry_base_ms.max(1),
        }
    }

    pub async fn get_agg_trades(
        &self,
        symbol: &str,
        start_time_ms: Option<i64>,
        end_time_ms: Option<i64>,
        from_id: Option<i64>,
        limit: Option<u16>,
    ) -> Result<Vec<BinanceAggTrade>, AppError> {
        if symbol.trim().is_empty() {
            return Err(AppError::Validation("symbol cannot be empty".into()));
        }

        let limit = limit.unwrap_or(1000).clamp(1, 1000);
        let url = format!("{}/api/v3/aggTrades", self.base_url);
        let mut attempts = 0u32;

        loop {
            let mut query: Vec<(&str, String)> = Vec::with_capacity(5);
            query.push(("symbol", symbol.to_string()));
            if from_id.is_some() {
                if let Some(fid) = from_id {
                    query.push(("fromId", fid.to_string()));
                }
            } else if let Some(start) = start_time_ms {
                query.push(("startTime", start.to_string()));
            }
            if let Some(end) = end_time_ms {
                query.push(("endTime", end.to_string()));
            }
            query.push(("limit", limit.to_string()));

            let response = self.http.get(&url).query(&query).send().await;

            let response = match response {
                Ok(resp) => resp,
                Err(err) => {
                    return Err(AppError::External(format!(
                        "failed to call binance aggTrades: {err}"
                    )));
                }
            };

            let status = response.status();
            if status == StatusCode::TOO_MANY_REQUESTS || status == StatusCode::IM_A_TEAPOT {
                if attempts >= self.retry_max {
                    return Err(AppError::External(format!(
                        "binance aggTrades rate limited after {} attempts",
                        attempts + 1
                    )));
                }

                let backoff_base = self
                    .retry_base_ms
                    .saturating_mul(1u64.saturating_shl(attempts.min(16)));
                let jitter = thread_rng().gen_range(0..=self.retry_base_ms);
                let delay_ms = backoff_base + jitter;
                warn!(
                    %symbol,
                    status = %status,
                    attempt = attempts + 1,
                    retry_in_ms = delay_ms,
                    "binance rate limited, backing off"
                );
                sleep(Duration::from_millis(delay_ms)).await;
                attempts += 1;
                continue;
            }

            if !status.is_success() {
                let body = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "<empty>".to_string());
                return Err(AppError::External(format!(
                    "binance aggTrades request failed ({status}): {body}"
                )));
            }

            let mut trades: Vec<BinanceAggTrade> = response
                .json()
                .await
                .map_err(|err| AppError::External(format!("invalid aggTrades payload: {err}")))?;

            trades.sort_by(|a, b| a.T.cmp(&b.T).then(a.a.cmp(&b.a)));

            return Ok(trades);
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct BinanceAggTrade {
    #[serde(rename = "a")]
    pub a: i64,
    #[serde(rename = "p", deserialize_with = "deserialize_f64_from_str")]
    pub p: f64,
    #[serde(rename = "q", deserialize_with = "deserialize_f64_from_str")]
    pub q: f64,
    #[serde(rename = "T")]
    pub T: i64,
    #[serde(rename = "m")]
    pub m: bool,
    #[serde(rename = "Q", deserialize_with = "deserialize_f64_from_str")]
    pub Q: f64,
}

fn deserialize_f64_from_str<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    struct F64Visitor;

    impl<'de> de::Visitor<'de> for F64Visitor {
        type Value = f64;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a string or number representing a float")
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E> {
            Ok(value)
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
            Ok(value as f64)
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
            Ok(value as f64)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            value
                .parse::<f64>()
                .map_err(|err| E::custom(format!("invalid float: {err}")))
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.visit_str(&value)
        }
    }

    deserializer.deserialize_any(F64Visitor)
}
