//! BTC 15-minute binary market discovery from Polymarket Gamma API.
//! Slug pattern: btc-updown-15m-{unix_timestamp}
//! Two markets per event: "Up" and "Down". One always resolves $1.00.

use crate::api::PolymarketApi;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BtcMarket {
    pub event_id: String,
    pub slug: String,
    pub title: String,
    pub end_time: DateTime<Utc>,
    pub up_token_id: String,
    pub down_token_id: String,
}

#[derive(Debug, Deserialize)]
struct GammaEvent {
    id: String,
    slug: Option<String>,
    title: Option<String>,
    #[serde(rename = "endDate")]
    end_date: Option<String>,
    markets: Option<Vec<GammaMarket>>,
}

#[derive(Debug, Deserialize)]
struct GammaMarket {
    question: Option<String>,
    slug: Option<String>,
    #[serde(rename = "clobTokenIds")]
    clob_token_ids: Option<serde_json::Value>,
}

pub struct BtcDiscovery {
    api: Arc<PolymarketApi>,
}

impl BtcDiscovery {
    pub fn new(api: Arc<PolymarketApi>) -> Self {
        Self { api }
    }

    /// Fetch all active BTC 15-min markets (endDate > now), sorted by endDate ascending.
    pub async fn fetch_active_markets(&self) -> Result<Vec<BtcMarket>> {
        let url = format!("{}/events?limit=200", self.api.gamma_url());
        let values = self.api.fetch_gamma_events(&url).await?;
        let now = Utc::now();
        let mut markets = Vec::new();

        for v in values {
            let ev: GammaEvent = match serde_json::from_value(v) {
                Ok(e) => e,
                Err(_) => continue,
            };
            let slug = ev.slug.as_deref().unwrap_or("");
            if !slug.contains("btc-updown-15m") {
                continue;
            }
            let end_time = match ev.end_date.as_deref().and_then(parse_datetime) {
                Some(dt) => dt,
                None => continue,
            };
            if end_time <= now {
                continue;
            }
            let mkt_list = match &ev.markets {
                Some(m) if m.len() >= 2 => m,
                _ => continue,
            };
            let (up_token, down_token) = extract_tokens(mkt_list);
            let (up_token, down_token) = match (up_token, down_token) {
                (Some(u), Some(d)) => (u, d),
                _ => continue,
            };
            markets.push(BtcMarket {
                event_id: ev.id,
                slug: slug.to_string(),
                title: ev.title.unwrap_or_default(),
                end_time,
                up_token_id: up_token,
                down_token_id: down_token,
            });
        }
        markets.sort_by_key(|m| m.end_time);
        Ok(markets)
    }
}

/// Detect Up and Down token IDs from market list by question/slug keywords.
/// Falls back to positional assignment if detection fails.
fn extract_tokens(markets: &[GammaMarket]) -> (Option<String>, Option<String>) {
    let mut up = None;
    let mut down = None;
    for m in markets {
        let q = m.question.as_deref().unwrap_or("").to_lowercase();
        let s = m.slug.as_deref().unwrap_or("").to_lowercase();
        let token = parse_token_id(m.clob_token_ids.as_ref());
        if q.contains("up") || s.contains("up") {
            up = token;
        } else if q.contains("down") || s.contains("down") {
            down = token;
        }
    }
    // Positional fallback
    if up.is_none() || down.is_none() {
        let tokens: Vec<_> = markets
            .iter()
            .filter_map(|m| parse_token_id(m.clob_token_ids.as_ref()))
            .collect();
        if up.is_none() {
            up = tokens.first().cloned();
        }
        if down.is_none() {
            down = tokens.get(1).cloned();
        }
    }
    (up, down)
}

fn parse_token_id(val: Option<&serde_json::Value>) -> Option<String> {
    let val = val?;
    if let Some(arr) = val.as_array() {
        return arr.first()?.as_str().map(String::from);
    }
    if let Some(s) = val.as_str() {
        let parsed: Vec<String> = serde_json::from_str(s).ok()?;
        return parsed.into_iter().next();
    }
    None
}

fn parse_datetime(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}
