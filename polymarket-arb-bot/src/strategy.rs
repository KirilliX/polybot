//! BTC 15-min spread arbitrage strategy.
//! Entry: buy Up + Down simultaneously when up_ask + down_ask < 1/(1+2*fee) - margin.
//! One leg always resolves $1.00; guaranteed profit if entry sum below breakeven.

use crate::api::PolymarketApi;
use crate::btc::{BtcDiscovery, BtcMarket};
use crate::config::Config;
use anyhow::Result;
use chrono::Utc;
use log::{error, info, warn};
use std::sync::Arc;
use tokio::time::{sleep, Duration};

pub struct BtcArbStrategy {
    api: Arc<PolymarketApi>,
    config: Config,
    discovery: BtcDiscovery,
}

impl BtcArbStrategy {
    pub fn new(api: Arc<PolymarketApi>, config: Config) -> Self {
        Self {
            discovery: BtcDiscovery::new(api.clone()),
            api,
            config,
        }
    }

    pub async fn run(&self) -> Result<()> {
        let fee = self.config.strategy.taker_fee;
        // Breakeven: (up+down) * (1 + 2*fee) < 1.0
        // So threshold = 1/(1+2*fee); subtract margin for safety buffer
        let buy_threshold = 1.0 / (1.0 + 2.0 * fee) - self.config.strategy.min_margin;
        let shares: f64 = self.config.strategy.shares.parse().unwrap_or(100.0);
        let sim = self.config.strategy.simulation_mode;
        let poll_ms = self.config.strategy.price_poll_interval_ms;
        let min_secs = self.config.strategy.min_seconds_before_expiry;

        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        info!("BTC 15-min Spread Arbitrage Bot");
        info!("  Fee: {:.1}% per leg | Threshold: {:.4} | Margin: {:.4}",
            fee * 100.0, buy_threshold, self.config.strategy.min_margin);
        info!("  Shares: {} | Poll: {}ms | Simulation: {}", shares, poll_ms, sim);
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        loop {
            let markets = match self.discovery.fetch_active_markets().await {
                Ok(m) if !m.is_empty() => m,
                Ok(_) => {
                    info!("No active BTC 15-min markets. Waiting 60s...");
                    sleep(Duration::from_secs(60)).await;
                    continue;
                }
                Err(e) => {
                    warn!("Failed to fetch markets: {}. Retrying in 30s.", e);
                    sleep(Duration::from_secs(30)).await;
                    continue;
                }
            };

            // Nearest market with enough time left to trade
            let market = markets.iter().find(|m| {
                (m.end_time - Utc::now()).num_seconds() > min_secs
            });

            let market = match market {
                Some(m) => m.clone(),
                None => {
                    let next_expiry = markets.first().map(|m| m.end_time);
                    info!("All markets expire too soon. Next: {:?}. Waiting 30s.", next_expiry);
                    sleep(Duration::from_secs(30)).await;
                    continue;
                }
            };

            let secs_left = (market.end_time - Utc::now()).num_seconds();
            info!("Target: {} | TTL: {}s | Ends: {}",
                market.title, secs_left,
                market.end_time.format("%H:%M:%S UTC"));

            if let Err(e) = self.run_market_cycle(
                &market, buy_threshold, shares, sim, poll_ms,
            ).await {
                error!("Cycle error for {}: {}", market.title, e);
            }

            sleep(Duration::from_secs(10)).await;
        }
    }

    async fn run_market_cycle(
        &self,
        market: &BtcMarket,
        buy_threshold: f64,
        shares: f64,
        sim: bool,
        poll_ms: u64,
    ) -> Result<()> {
        let mut bought = false;
        let mut entry_sum = 0.0;

        loop {
            let secs_left = (market.end_time - Utc::now()).num_seconds();
            if secs_left <= 0 {
                if bought {
                    info!("Market {} expired. Entry: {:.4}. Expected profit: {:.4} per share.",
                        market.slug, entry_sum, 1.0 - entry_sum);
                    info!("Use --redeem to claim winning tokens after resolution.");
                } else {
                    info!("Market {} expired without entry.", market.slug);
                }
                break;
            }

            let (up_res, down_res) = tokio::join!(
                self.api.get_best_ask(&market.up_token_id),
                self.api.get_best_ask(&market.down_token_id),
            );

            match (up_res.ok().flatten(), down_res.ok().flatten()) {
                (Some(up), Some(down)) => {
                    let sum = up + down;
                    let gap = buy_threshold - sum;
                    info!("Up: {:.4} | Down: {:.4} | Sum: {:.4} | Gap: {:.4} | TTL: {}s{}",
                        up, down, sum, gap, secs_left,
                        if bought { " [HOLDING]" } else { "" });

                    if !bought && sum < buy_threshold {
                        info!("ENTRY: sum {:.4} < threshold {:.4}. Buying {} shares Up + Down...",
                            sum, buy_threshold, shares);

                        if sim {
                            info!("[SIM] Up @ {:.4} + Down @ {:.4} | Cost: {:.4} | Profit: {:.4}",
                                up, down, sum, 1.0 - sum);
                            bought = true;
                            entry_sum = sum;
                        } else {
                            let (r_up, r_down) = tokio::join!(
                                self.api.place_market_order(&market.up_token_id, shares, "BUY", Some("FAK")),
                                self.api.place_market_order(&market.down_token_id, shares, "BUY", Some("FAK")),
                            );
                            let up_ok = r_up.is_ok();
                            let down_ok = r_down.is_ok();
                            if let Err(e) = r_up { error!("Up order failed: {}", e); }
                            if let Err(e) = r_down { error!("Down order failed: {}", e); }
                            if up_ok && down_ok {
                                info!("Bought Up + Down. Cost: {:.4} | Expected profit: {:.4}/share",
                                    sum, 1.0 - sum);
                                bought = true;
                                entry_sum = sum;
                            }
                        }
                    }
                }
                _ => {
                    warn!("Price fetch failed for {}. TTL: {}s", market.slug, secs_left);
                }
            }

            sleep(Duration::from_millis(poll_ms)).await;
        }

        Ok(())
    }
}
