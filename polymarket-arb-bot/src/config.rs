use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about = "Polymarket BTC 15-min Spread Arbitrage Bot")]
pub struct Args {
    #[arg(short, long, default_value = "config.json")]
    pub config: PathBuf,

    /// Redeem winning tokens for completed markets
    #[arg(long)]
    pub redeem: bool,

    /// Specific condition ID to redeem (optional; without it redeems all redeemable)
    #[arg(long, requires = "redeem")]
    pub condition_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub polymarket: PolymarketConfig,
    pub strategy: StrategyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    /// Taker fee per leg (e.g. 0.02 = 2%). Breakeven: up+down < 1/(1+2*fee).
    #[serde(default = "default_taker_fee")]
    pub taker_fee: f64,
    /// Safety margin subtracted from breakeven threshold (e.g. 0.005).
    #[serde(default = "default_min_margin")]
    pub min_margin: f64,
    /// Shares per leg when buying (Up and Down).
    #[serde(default = "default_shares")]
    pub shares: String,
    /// How often to poll Up/Down prices (milliseconds).
    #[serde(default = "default_price_poll_interval_ms")]
    pub price_poll_interval_ms: u64,
    /// How often to refresh market list from Gamma API (seconds).
    #[serde(default = "default_market_refresh_interval_secs")]
    pub market_refresh_interval_secs: u64,
    /// Don't enter a market if fewer than this many seconds remain until expiry.
    #[serde(default = "default_min_seconds_before_expiry")]
    pub min_seconds_before_expiry: i64,
    /// If true: log what would happen without placing real orders.
    #[serde(default)]
    pub simulation_mode: bool,
}

fn default_taker_fee() -> f64 { 0.02 }
fn default_min_margin() -> f64 { 0.005 }
fn default_shares() -> String { "100".to_string() }
fn default_price_poll_interval_ms() -> u64 { 5_000 }
fn default_market_refresh_interval_secs() -> u64 { 60 }
fn default_min_seconds_before_expiry() -> i64 { 120 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolymarketConfig {
    pub gamma_api_url: String,
    pub clob_api_url: String,
    pub api_key: Option<String>,
    pub api_secret: Option<String>,
    pub api_passphrase: Option<String>,
    pub private_key: Option<String>,
    pub proxy_wallet_address: Option<String>,
    pub signature_type: Option<u8>,
    #[serde(default)]
    pub rpc_url: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            polymarket: PolymarketConfig {
                gamma_api_url: "https://gamma-api.polymarket.com".to_string(),
                clob_api_url: "https://clob.polymarket.com".to_string(),
                api_key: None,
                api_secret: None,
                api_passphrase: None,
                private_key: None,
                proxy_wallet_address: None,
                signature_type: None,
                rpc_url: None,
            },
            strategy: StrategyConfig {
                taker_fee: default_taker_fee(),
                min_margin: default_min_margin(),
                shares: default_shares(),
                price_poll_interval_ms: default_price_poll_interval_ms(),
                market_refresh_interval_secs: default_market_refresh_interval_secs(),
                min_seconds_before_expiry: default_min_seconds_before_expiry(),
                simulation_mode: true,
            },
        }
    }
}

impl Config {
    pub fn load(path: &PathBuf) -> anyhow::Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            let config = Config::default();
            std::fs::write(path, serde_json::to_string_pretty(&config)?)?;
            log::info!("Created default config at {:?}", path);
            Ok(config)
        }
    }
}
