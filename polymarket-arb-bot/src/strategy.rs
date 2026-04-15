//! Soccer match trading: buy high-priced team + Draw when sum < threshold;
//! monitor via API polling; risk management (sell team, buy opposite) or profit exit (sell both).

use crate::api::PolymarketApi;
use crate::config::Config;
use crate::models::Position;
use crate::soccer::{SoccerDiscovery, SoccerMatch, SoccerOutcome};
use chrono::Utc;
use anyhow::Result;
use log::{error, info, warn};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{RwLock, Semaphore};
use tokio::time::{sleep, Duration};

/// API price poll interval (ms). Replaces WebSocket for reliable price data.
const PRICE_POLL_MS: u64 = 200;
const LIVE_PRICE_LOG_INTERVAL_SECS: u64 = 1;
const MIN_ORDER_USD: f64 = 1.0;

#[derive(Debug, Clone)]
pub struct HoldingPosition {
    pub match_slug: String,
    pub team_token_id: String,
    pub team_outcome: SoccerOutcome,
    /// None = single-team holding (rising-team buy). Some = team+draw holding.
    pub draw_token_id: Option<String>,
    pub buy_price_team: f64,
    pub buy_price_draw: f64,
    pub size: f64,
}

pub struct SoccerStrategy {
    api: Arc<PolymarketApi>,
    config: Config,
    discovery: SoccerDiscovery,
}

impl SoccerStrategy {
    pub fn new(api: Arc<PolymarketApi>, config: Config) -> Self {
        Self {
            discovery: SoccerDiscovery::new(api.clone()),
            api,
            config,
        }
    }

    pub async fn run(&self) -> Result<()> {
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        info!("Soccer match trading bot");
        info!(
            "  Buy when (team + draw) < {} | Risk threshold: {}% | Profit exit: >= {}",
            self.config.strategy.buy_threshold,
            self.config.strategy.risk_reduce_threshold * 100.0,
            self.config.strategy.sell_profit_threshold
        );
        info!("  Poll: {}s | Cache refresh: {}s | Live window: {}m | Max concurrent: {} | Shares: {}",
            self.config.strategy.market_poll_interval_secs,
            self.config.strategy.market_refresh_interval_secs,
            self.config.strategy.live_window_minutes,
            self.config.strategy.max_concurrent_markets,
            self.config.strategy.shares);
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        // Log holding stats at startup (balance, allowance, positions)
        self.log_holding_stats().await;

        let last_trade_at: Arc<RwLock<Option<std::time::Instant>>> = Arc::new(RwLock::new(None));
        let mut cached_matches: Vec<SoccerMatch> = Vec::new();
        let cache_path = &self.config.strategy.market_cache_path;
        let refresh_secs = self.config.strategy.market_refresh_interval_secs;
        let live_window = self.config.strategy.live_window_minutes;
        let mut last_refresh = std::time::Instant::now(); // First iter: is_empty triggers fetch
        let max_concurrent = self.config.strategy.max_concurrent_markets as usize;
        let (semaphore, trading_ids) = if max_concurrent > 1 {
            (
                Arc::new(Semaphore::new(max_concurrent)),
                Arc::new(RwLock::new(HashSet::new())),
            )
        } else {
            (Arc::new(Semaphore::new(0)), Arc::new(RwLock::new(HashSet::new())))
        };

        loop {
            let should_refresh = last_refresh.elapsed().as_secs() >= refresh_secs || cached_matches.is_empty();
            if should_refresh {
                match self.discovery.fetch_sort_and_save(
                    &self.config.strategy.soccer_tag_ids,
                    self.config.strategy.market_fetch_limit,
                    cache_path,
                    live_window,
                ).await {
                    Ok(matches) => {
                        cached_matches = matches;
                        last_refresh = std::time::Instant::now();
                    }
                    Err(e) => {
                        warn!("Failed to fetch soccer matches: {}.", e);
                        if cached_matches.is_empty() {
                            if let Ok(matches) = SoccerDiscovery::load_cached_matches(cache_path) {
                                cached_matches = matches;
                                info!("Loaded {} matches from cache", cached_matches.len());
                            }
                        }
                    }
                }
            }

            // Periodic holding stats (same interval as market refresh)
            if should_refresh {
                self.log_holding_stats().await;
            }

            let (held_matches, held_positions_map) = self.select_matches_with_holdings(&cached_matches).await;

            // Always fetch tradable so we can run up to max_concurrent (held + tradable in parallel)
            let tradable_matches = if cached_matches.is_empty() {
                Vec::new()
            } else {
                self.select_tradable_live_matches(
                    &cached_matches,
                    live_window,
                    max_concurrent,
                ).await
            };

            // Held first (monitor positions), then tradable; dedup; take up to max_concurrent
            let mut matches_to_run: Vec<SoccerMatch> = Vec::new();
            let mut seen = HashSet::new();
            for held in &held_matches {
                if !seen.contains(&held.event_id) {
                    seen.insert(held.event_id.clone());
                    matches_to_run.push(held.clone());
                }
            }
            for m in &tradable_matches {
                if !seen.contains(&m.event_id) && matches_to_run.len() < max_concurrent {
                    seen.insert(m.event_id.clone());
                    matches_to_run.push(m.clone());
                }
            }

            if !matches_to_run.is_empty() {
                if max_concurrent == 1 {
                    let live_match = matches_to_run.first().unwrap();
                    let initial = held_positions_map.get(&live_match.event_id).cloned();
                    info!("Live match: {} (game_start: {:?}){}", live_match.title, live_match.game_start,
                        if initial.is_some() { " [monitoring existing holdings]" } else { "" });
                    if let Err(e) = self.run_match_cycle(live_match.clone(), Arc::clone(&last_trade_at), initial).await {
                        error!("Match cycle error: {}", e);
                    }
                } else {
                    for live_match in &matches_to_run {
                        let ids = trading_ids.write().await;
                        if ids.contains(&live_match.event_id) {
                            continue;
                        }
                        drop(ids);

                        let permit = match semaphore.clone().try_acquire_owned() {
                            Ok(p) => p,
                            Err(_) => break,
                        };

                        let m_clone = live_match.clone();
                        let event_id = m_clone.event_id.clone();
                        let initial = held_positions_map.get(&event_id).cloned();
                        let api = self.api.clone();
                        let config = self.config.clone();
                        let last_ta = Arc::clone(&last_trade_at);
                        let trading_ids_clone = Arc::clone(&trading_ids);

                        trading_ids.write().await.insert(event_id.clone());

                        tokio::spawn(async move {
                            let _permit = permit;
                            if let Err(e) = Self::run_match_cycle_static(
                                api,
                                config,
                                m_clone,
                                last_ta,
                                initial,
                            ).await {
                                error!("Match cycle error: {}", e);
                            }
                            trading_ids_clone.write().await.remove(&event_id);
                        });

                        info!("Started trading: {} ({} active)", live_match.title, trading_ids.read().await.len());
                    }
                }
            } else {
                let live_skipped = self.discovery.select_live_matches(
                    &cached_matches,
                    live_window,
                    self.config.strategy.trading_start_minutes_before,
                    self.config.strategy.min_minutes_into_game,
                    10,
                );
                if !live_skipped.is_empty() {
                    info!("{} live match(es) in progress (skipped: sum >= buy_threshold {})", live_skipped.len(), self.config.strategy.buy_threshold);
                    for m in live_skipped.iter().take(5) {
                        let (ah, ad, aa) = tokio::join!(
                            self.api.get_best_ask(&m.home_market.yes_token_id),
                            self.api.get_best_ask(&m.draw_market.yes_token_id),
                            self.api.get_best_ask(&m.away_market.yes_token_id),
                        );
                        let (h, d, a) = (
                            ah.ok().flatten(),
                            ad.ok().flatten(),
                            aa.ok().flatten(),
                        );
                        let (h_str, d_str, a_str) = (
                            h.map(|v| format!("${:.2}", v)).unwrap_or_else(|| "N/A".into()),
                            d.map(|v| format!("${:.2}", v)).unwrap_or_else(|| "N/A".into()),
                            a.map(|v| format!("${:.2}", v)).unwrap_or_else(|| "N/A".into()),
                        );
                        let sum = match (h, d, a) {
                            (Some(hi), Some(di), Some(ai)) => Some((hi + di).max(ai + di)),
                            _ => None,
                        };
                        let sum_str = sum.map(|s| format!("{:.2}", s)).unwrap_or_else(|| "N/A".into());
                        let gs = m.game_start
                            .map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string())
                            .unwrap_or_else(|| "?".into());
                        info!("  • {} @ {} | H {} D {} A {} | sum {}", m.title, gs, h_str, d_str, a_str, sum_str);
                    }
                }
                let upcoming = self.discovery.select_nearest_upcoming(&cached_matches, self.config.strategy.nearest_upcoming_count);
                if upcoming.is_empty() {
                    info!("No tradable matches. No upcoming in cache. Waiting {}s.", self.config.strategy.market_poll_interval_secs);
                } else {
                    let now = chrono::Utc::now();
                    info!("Current time: {} UTC", now.format("%Y-%m-%d %H:%M:%S"));
                    info!("Nearest upcoming ({}):", upcoming.len());
                    for u in upcoming {
                        let (gs, remaining) = u.game_start
                            .map(|dt| {
                                let r = (dt - now).to_std().unwrap_or_default();
                                let secs = r.as_secs();
                                let days = secs / 86400;
                                let hrs = (secs % 86400) / 3600;
                                let mins = (secs % 3600) / 60;
                                let rem = if days > 0 {
                                    format!("in {}d {}h {}m", days, hrs, mins)
                                } else if hrs > 0 {
                                    format!("in {}h {}m", hrs, mins)
                                } else {
                                    format!("in {}m", mins)
                                };
                                (dt.format("%Y-%m-%d %H:%M UTC").to_string(), rem)
                            })
                            .unwrap_or_else(|| ("?".into(), "?".into()));
                        info!("  • {} @ {} ({})", u.title, gs, remaining);
                    }
                    info!("Waiting {}s.", self.config.strategy.market_poll_interval_secs);
                }
            }

            sleep(Duration::from_secs(self.config.strategy.market_poll_interval_secs)).await;
        }
    }

    /// Log USDC balance, allowance, and position stats (for "not enough balance" diagnostics).
    async fn log_holding_stats(&self) {
        let wallet = match self.api.get_trading_wallet() {
            Ok(w) => w,
            Err(e) => {
                warn!("Could not get trading wallet for holding stats: {}", e);
                return;
            }
        };
        let (balance_res, positions_res) = tokio::join!(
            self.api.get_balance_allowance(&wallet),
            self.api.get_all_positions(&wallet),
        );
        let balance = balance_res.unwrap_or_default();
        let positions = positions_res.unwrap_or_default();
        let active: Vec<_> = positions.iter().filter(|p| p.size > 0.0 && !p.redeemable.unwrap_or(false)).collect();
        let total_val: f64 = active.iter()
            .map(|p| p.size * p.avg_price.unwrap_or(0.0))
            .sum();
        info!("📊 Holding stats | Wallet: {}..{} | USDC: ${:.2} | Allowance: ${:.2} | Positions: {} ({} active) | ~Value: ${:.2}",
            &wallet[..2.min(wallet.len())], &wallet[wallet.len().saturating_sub(6)..],
            balance.balance_usdc, balance.allowance_usdc,
            positions.len(), active.len(), total_val);
        for p in active.iter().take(5) {
            let title = p.title.as_deref().unwrap_or("?");
            let ev = p.event_id.as_deref().unwrap_or("?");
            info!("   • {} (event {}) | {} shares @ ${:.2}", title, ev, p.size, p.avg_price.unwrap_or(0.0));
        }
        if active.len() > 5 {
            info!("   ... and {} more", active.len() - 5);
        }
    }

    /// Select matches where we hold tokens (team+draw). Returns matches and reconstructed HoldingPosition per event.
    async fn select_matches_with_holdings(
        &self,
        cached_matches: &[SoccerMatch],
    ) -> (Vec<SoccerMatch>, HashMap<String, HoldingPosition>) {
        let wallet = match self.api.get_trading_wallet() {
            Ok(w) => w,
            Err(_) => return (Vec::new(), HashMap::new()),
        };
        let positions = match self.api.get_all_positions(&wallet).await {
            Ok(p) => p,
            Err(_) => return (Vec::new(), HashMap::new()),
        };
        let active: Vec<_> = positions
            .into_iter()
            .filter(|p| p.size > 0.0 && !p.redeemable.unwrap_or(false))
            .collect();
        let by_event: HashMap<String, Vec<Position>> = {
            let mut m = HashMap::new();
            for p in active {
                if let Some(eid) = p.event_id.as_ref() {
                    m.entry(eid.clone()).or_insert_with(Vec::new).push(p);
                }
            }
            m
        };
        let mut held_matches = Vec::new();
        let mut held_positions = HashMap::new();
        for m in cached_matches {
            let Some(positions) = by_event.get(&m.event_id) else { continue };
            let (draw_pos, team_pos) = {
                let mut draw = None;
                let mut team = None;
                for p in positions {
                    let slug = p.slug.as_deref().unwrap_or("");
                    if slug.to_lowercase().contains("draw") {
                        draw = Some(p);
                    } else {
                        team = Some(p);
                    }
                }
                (draw, team)
            };
            let Some(team_pos) = team_pos else { continue };
            let team_outcome = if team_pos.token_id == m.home_market.yes_token_id {
                SoccerOutcome::HomeWin
            } else if team_pos.token_id == m.away_market.yes_token_id {
                SoccerOutcome::AwayWin
            } else {
                continue;
            };
            let (draw_token_id, buy_price_draw, size) = match &draw_pos {
                Some(d) => (Some(d.token_id.clone()), d.avg_price.unwrap_or(0.0), team_pos.size.min(d.size)),
                None => (None, 0.0, team_pos.size),
            };
            let holding = HoldingPosition {
                match_slug: m.slug.clone(),
                team_token_id: team_pos.token_id.clone(),
                team_outcome,
                draw_token_id,
                buy_price_team: team_pos.avg_price.unwrap_or(0.0),
                buy_price_draw,
                size,
            };
            held_positions.insert(m.event_id.clone(), holding);
            held_matches.push(m.clone());
        }
        (held_matches, held_positions)
    }

    /// Select live matches where: (higher team + draw) sum < buy_threshold,
    /// OR team price diff < min_team_price_diff (monitor for rising-team buy).
    async fn select_tradable_live_matches(
        &self,
        matches: &[SoccerMatch],
        live_window_minutes: i64,
        max_count: usize,
    ) -> Vec<SoccerMatch> {
        let live = self.discovery.select_live_matches(
            matches,
            live_window_minutes,
            self.config.strategy.trading_start_minutes_before,
            self.config.strategy.min_minutes_into_game,
            50,
        );
        let buy_threshold = self.config.strategy.buy_threshold;
        let min_diff = self.config.strategy.min_team_price_diff;
        let window = chrono::Duration::minutes(live_window_minutes);
        let mut with_score: Vec<(f64, SoccerMatch)> = Vec::new();
        for m in live {
            let (ah, ad, aa) = tokio::join!(
                self.api.get_best_ask(&m.home_market.yes_token_id),
                self.api.get_best_ask(&m.draw_market.yes_token_id),
                self.api.get_best_ask(&m.away_market.yes_token_id),
            );
            let (h, d, a) = (
                ah.ok().flatten(),
                ad.ok().flatten(),
                aa.ok().flatten(),
            );
            let (sum, team_diff) = match (h, d, a) {
                (Some(hi), Some(di), Some(ai)) => {
                    let sum_home_draw = hi + di;
                    let sum_away_draw = ai + di;
                    let sum_val = sum_home_draw.max(sum_away_draw);
                    let diff = hi.max(ai) - hi.min(ai);
                    (sum_val, diff)
                }
                _ => continue,
            };
            let tradable = sum < buy_threshold || team_diff < min_diff;
            if tradable {
                with_score.push((sum, m.clone()));
            }
        }
        with_score.sort_by(|a, b| {
            let a_end = a.1.game_start.unwrap_or(chrono::DateTime::<Utc>::MAX_UTC) + window;
            let b_end = b.1.game_start.unwrap_or(chrono::DateTime::<Utc>::MAX_UTC) + window;
            a_end.cmp(&b_end)
        });
        with_score.into_iter().take(max_count).map(|(_, m)| m).collect()
    }

    async fn run_match_cycle_static(
        api: Arc<PolymarketApi>,
        config: Config,
        m: SoccerMatch,
        last_trade_at: Arc<RwLock<Option<std::time::Instant>>>,
        initial_holding: Option<HoldingPosition>,
    ) -> Result<()> {
        if m.closed {
            return Ok(());
        }

        let buy_threshold = config.strategy.buy_threshold;
        let min_diff = config.strategy.min_team_price_diff;
        // When we have existing holdings, skip the "sum >= threshold" exit (we're monitoring to sell)
        if initial_holding.is_none() {
            let (ah, ad, aa) = tokio::join!(
                api.get_best_ask(&m.home_market.yes_token_id),
                api.get_best_ask(&m.draw_market.yes_token_id),
                api.get_best_ask(&m.away_market.yes_token_id),
            );
            let (h, d, a) = (
                ah.ok().flatten(),
                ad.ok().flatten(),
                aa.ok().flatten(),
            );
            if let (Some(hi), Some(di), Some(ai)) = (h, d, a) {
                info!(
                    "Live prices {} | Home Yes ${:.2} Draw Yes ${:.2} Away Yes ${:.2} (diff {:.2})",
                    m.title, hi, di, ai, hi.max(ai) - hi.min(ai)
                );
                let sum = (hi + di).max(ai + di);
                let team_diff = hi.max(ai) - hi.min(ai);
                // Exit only if: sum too high AND strong favorite (diff >= min) — no monitoring value
                if sum >= buy_threshold && team_diff >= min_diff {
                    info!(
                        "Skipping {}: live sum {:.2} >= buy_threshold {} and diff {:.2} >= {} — exiting",
                        m.title, sum, buy_threshold, team_diff, min_diff
                    );
                    return Ok(());
                }
            }
        }

        let mut holding: Option<HoldingPosition> = initial_holding;
        let mut last_price_log = std::time::Instant::now();
        let buy_threshold = config.strategy.buy_threshold;
        let live_window = chrono::Duration::minutes(config.strategy.live_window_minutes);
        let risk_threshold = config.strategy.risk_reduce_threshold;
        let sell_threshold = config.strategy.sell_profit_threshold;
        let shares_f: f64 = config.strategy.shares.parse().unwrap_or(100.0);
        let simulation = config.strategy.simulation_mode;
        let interval_secs = config.strategy.trade_interval_secs;
        let min_diff = config.strategy.min_team_price_diff;
        let rising_pct = config.strategy.rising_buy_threshold_pct;
        let single_profit_pct = config.strategy.single_team_profit_rise_pct;
        let stale_polls = config.strategy.stale_price_data_points;
        let mut prev_home: Option<f64> = None;
        let mut prev_away: Option<f64> = None;
        let mut stale_count: u32 = 0;
        let mut last_team_price: Option<f64> = None;

        loop {
            if m.closed {
                break;
            }
            let match_ended = if let Some(gs) = m.game_start {
                chrono::Utc::now() > gs + live_window
            } else {
                m.end_date.map(|ed| chrono::Utc::now() > ed).unwrap_or(false)
            };
            if match_ended {
                info!("Match {} ended", m.title);
                break;
            }

            // Fetch ask and bid prices via API (200ms poll interval) — always both during monitoring
            let (ah, ad, aa, bh, bd, ba) = tokio::join!(
                api.get_best_ask(&m.home_market.yes_token_id),
                api.get_best_ask(&m.draw_market.yes_token_id),
                api.get_best_ask(&m.away_market.yes_token_id),
                api.get_best_bid(&m.home_market.yes_token_id),
                api.get_best_bid(&m.draw_market.yes_token_id),
                api.get_best_bid(&m.away_market.yes_token_id),
            );
            let ask_home = ah.ok().flatten();
            let ask_draw = ad.ok().flatten();
            let ask_away = aa.ok().flatten();
            let bid_home = bh.ok().flatten();
            let bid_draw = bd.ok().flatten();
            let bid_away = ba.ok().flatten();

            if last_price_log.elapsed().as_secs() >= LIVE_PRICE_LOG_INTERVAL_SECS {
                let (h, d, a) = (ask_home, ask_draw, ask_away);
                let (bh, bd, ba) = (bid_home, bid_draw, bid_away);
                info!(
                    "Live prices: {} Home ask/bid ${:.2}/${:.2} Draw ${:.2}/${:.2} Away ${:.2}/${:.2}",
                    m.title,
                    h.unwrap_or(0.0), bh.unwrap_or(0.0),
                    d.unwrap_or(0.0), bd.unwrap_or(0.0),
                    a.unwrap_or(0.0), ba.unwrap_or(0.0)
                );
                last_price_log = std::time::Instant::now();
            }

            if let Some(ref pos) = holding {
                let (team_ask, opp_ask, team_bid) = match pos.team_outcome {
                    SoccerOutcome::HomeWin => (ask_home, ask_away, bid_home),
                    SoccerOutcome::AwayWin => (ask_away, ask_home, bid_away),
                    _ => (None, None, None),
                };

                // Single-team holding (rising-team buy): profit or stale exit
                if pos.draw_token_id.is_none() {
                    if let Some(ta) = team_ask {
                        // Stale: no price change for N polls
                        let prev = last_team_price.replace(ta);
                        if prev.map(|p| (p - ta).abs() < 1e-6).unwrap_or(false) {
                            stale_count += 1;
                        } else {
                            stale_count = 0;
                        }
                        if stale_count >= stale_polls {
                            info!("Stale exit: {} no price change for {} polls — selling single team", m.title, stale_polls);
                            let sell_token = match pos.team_outcome {
                                SoccerOutcome::HomeWin => &m.home_market.yes_token_id,
                                SoccerOutcome::AwayWin => &m.away_market.yes_token_id,
                                _ => { sleep(Duration::from_millis(PRICE_POLL_MS)).await; continue; },
                            };
                            if !simulation {
                                let _ = api.place_market_order(sell_token, shares_f, "SELL", Some("FAK")).await;
                            }
                            holding = None;
                            *last_trade_at.write().await = Some(std::time::Instant::now());
                            break;
                        }
                        // Profit: price rose >= single_profit_pct from buy
                        let rise = (ta - pos.buy_price_team) / pos.buy_price_team;
                        if rise >= single_profit_pct {
                            let sell_token = match pos.team_outcome {
                                SoccerOutcome::HomeWin => &m.home_market.yes_token_id,
                                SoccerOutcome::AwayWin => &m.away_market.yes_token_id,
                                _ => { sleep(Duration::from_millis(PRICE_POLL_MS)).await; continue; },
                            };
                            // Use pre-fetched bid (fetched every poll during monitoring)
                            if let Some(bid) = team_bid {
                                if bid < pos.buy_price_team {
                                    info!("Single-team profit: bid ${:.4} < buy ${:.4} — skipping sell, keep monitoring", bid, pos.buy_price_team);
                                    sleep(Duration::from_millis(PRICE_POLL_MS)).await;
                                    continue;
                                }
                            } else {
                                info!("Single-team profit: no bid — skipping sell, keep monitoring");
                                sleep(Duration::from_millis(PRICE_POLL_MS)).await;
                                continue;
                            }
                            info!("Single-team profit exit: {} price rose {:.1}% >= {:.0}% — selling", m.title, rise * 100.0, single_profit_pct * 100.0);
                            if !simulation {
                                let _ = api.place_market_order(sell_token, shares_f, "SELL", Some("FAK")).await;
                            }
                            holding = None;
                            *last_trade_at.write().await = Some(std::time::Instant::now());
                            break;
                        }
                    }
                    sleep(Duration::from_millis(PRICE_POLL_MS)).await;
                    continue;
                }

                // Team+draw holding: profit or risk exit
                let draw_ask = ask_draw;
                if let (Some(ta), Some(da)) = (team_ask, draw_ask) {
                    let current_sum = ta + da;
                    let buy_sum = pos.buy_price_team + pos.buy_price_draw;
                    let reduction = 1.0 - (current_sum / buy_sum);

                    if current_sum >= sell_threshold {
                        let sell_token = match pos.team_outcome {
                            SoccerOutcome::HomeWin => &m.home_market.yes_token_id,
                            SoccerOutcome::AwayWin => &m.away_market.yes_token_id,
                            _ => continue,
                        };
                        let draw_id = pos.draw_token_id.as_ref().unwrap();
                        // Use pre-fetched bids (fetched every poll). Only sell if bid_team + bid_draw >= sell_profit_threshold
                        let bid_sum = match (team_bid, bid_draw) {
                            (Some(bt), Some(bd)) => bt + bd,
                            _ => {
                                info!("Profit exit: no bid for team or draw — skipping sell, keep monitoring");
                                sleep(Duration::from_millis(PRICE_POLL_MS)).await;
                                continue;
                            }
                        };
                        if bid_sum < sell_threshold {
                            info!("Profit exit: bid sum {:.4} < {} — skipping sell, keep monitoring", bid_sum, sell_threshold);
                            sleep(Duration::from_millis(PRICE_POLL_MS)).await;
                            continue;
                        }
                        info!("Profit exit: {} sum {:.4} >= {} — selling both", m.title, current_sum, sell_threshold);
                        if !simulation {
                            let _ = api.place_market_order(sell_token, shares_f, "SELL", Some("FAK")).await;
                            let _ = api.place_market_order(draw_id, shares_f, "SELL", Some("FAK")).await;
                            info!("Sold both for {}", m.title);
                        }
                        holding = None;
                        *last_trade_at.write().await = Some(std::time::Instant::now());
                        break;
                    } else if reduction >= risk_threshold {
                        info!(
                            "Risk management: {} price drop {:.1}% >= {:.1}% — sell team, buy opposite",
                            m.title, reduction * 100.0, risk_threshold * 100.0
                        );
                        if !simulation {
                            let (sell_token, buy_token) = match pos.team_outcome {
                                SoccerOutcome::HomeWin => (&m.home_market.yes_token_id, &m.away_market.yes_token_id),
                                SoccerOutcome::AwayWin => (&m.away_market.yes_token_id, &m.home_market.yes_token_id),
                                _ => continue,
                            };
                            let do_buy = opp_ask.unwrap_or(0.5) >= MIN_ORDER_USD / shares_f;
                            let _ = api.place_market_order(sell_token, shares_f, "SELL", Some("FAK")).await;
                            if do_buy {
                                let _ = api.place_market_order(buy_token, shares_f, "BUY", Some("FAK")).await;
                            }
                            if do_buy {
                                holding = Some(HoldingPosition {
                                    match_slug: m.slug.clone(),
                                    team_token_id: buy_token.to_string(),
                                    team_outcome: match &pos.team_outcome {
                                        SoccerOutcome::HomeWin => SoccerOutcome::AwayWin,
                                        SoccerOutcome::AwayWin => SoccerOutcome::HomeWin,
                                        x => x.clone(),
                                    },
                                    draw_token_id: pos.draw_token_id.clone(),
                                    buy_price_team: opp_ask.unwrap_or(0.5),
                                    buy_price_draw: pos.buy_price_draw,
                                    size: shares_f,
                                });
                                info!("Risk exit: transformed to (opposite + draw) for {}", m.title);
                            } else {
                                holding = None;
                            }
                        } else {
                            let (buy_token, new_outcome) = match pos.team_outcome {
                                SoccerOutcome::HomeWin => (&m.away_market.yes_token_id, SoccerOutcome::AwayWin),
                                SoccerOutcome::AwayWin => (&m.home_market.yes_token_id, SoccerOutcome::HomeWin),
                                _ => continue,
                            };
                            holding = Some(HoldingPosition {
                                match_slug: m.slug.clone(),
                                team_token_id: buy_token.to_string(),
                                team_outcome: new_outcome,
                                draw_token_id: pos.draw_token_id.clone(),
                                buy_price_team: opp_ask.unwrap_or(0.5),
                                buy_price_draw: pos.buy_price_draw,
                                size: shares_f,
                            });
                            info!("[SIM] Risk exit: transformed to (opposite + draw) for {} — continuing to monitor", m.title);
                        }
                        *last_trade_at.write().await = Some(std::time::Instant::now());
                        break;
                    }
                }
                sleep(Duration::from_millis(PRICE_POLL_MS)).await;
                continue;
            }

            let game_started = m
                .game_start
                .map(|gs| chrono::Utc::now() >= gs)
                .unwrap_or(true);
            if !game_started {
                sleep(Duration::from_millis(PRICE_POLL_MS)).await;
                continue;
            }

            if let Some(t) = *last_trade_at.read().await {
                if t.elapsed().as_secs() < interval_secs {
                    sleep(Duration::from_millis(PRICE_POLL_MS)).await;
                    continue;
                }
            }

            let (ask_home, ask_draw, ask_away) = (
                ask_home.unwrap_or(1.0),
                ask_draw.unwrap_or(1.0),
                ask_away.unwrap_or(1.0),
            );

            let sum_home_draw = ask_home + ask_draw;
            let sum_away_draw = ask_away + ask_draw;
            let team_diff = ask_home.max(ask_away) - ask_home.min(ask_away);
            let sum = sum_home_draw.max(sum_away_draw);

            if sum >= buy_threshold && team_diff >= min_diff {
                // Strong diff and sum too high — nothing to buy or monitor
                info!(
                    "Exiting {}: live sum {:.2} >= buy_threshold {} and diff {:.2} >= {} — no monitoring value",
                    m.title, sum, buy_threshold, team_diff, min_diff
                );
                break;
            }

            // Weak diff (team_diff < min_diff): monitor for rising team, don't buy team+draw
            if team_diff < min_diff {
                let home_rose = prev_home
                    .map(|ph| ph > 1e-9 && (ask_home - ph) / ph >= rising_pct)
                    .unwrap_or(false);
                let away_rose = prev_away
                    .map(|pa| pa > 1e-9 && (ask_away - pa) / pa >= rising_pct)
                    .unwrap_or(false);
                prev_home = Some(ask_home);
                prev_away = Some(ask_away);

                if home_rose || away_rose {
                    let (buy_token, buy_price, outcome) = if home_rose && (!away_rose || ask_home >= ask_away) {
                        (&m.home_market.yes_token_id, ask_home, SoccerOutcome::HomeWin)
                    } else {
                        (&m.away_market.yes_token_id, ask_away, SoccerOutcome::AwayWin)
                    };
                    if buy_price >= MIN_ORDER_USD / shares_f {
                        if simulation {
                            info!("[SIM] Rising-team buy: {} @ ${:.4} ({} shares, no draw)", m.title, buy_price, shares_f);
                        } else {
                            let _ = api.place_market_order(buy_token, shares_f, "BUY", Some("FAK")).await;
                            info!("Bought rising team {} @ ${:.4} for {}", m.title, buy_price, shares_f);
                        }
                        holding = Some(HoldingPosition {
                            match_slug: m.slug.clone(),
                            team_token_id: buy_token.to_string(),
                            team_outcome: outcome,
                            draw_token_id: None,
                            buy_price_team: buy_price,
                            buy_price_draw: 0.0,
                            size: shares_f,
                        });
                        *last_trade_at.write().await = Some(std::time::Instant::now());
                    }
                }
                sleep(Duration::from_millis(PRICE_POLL_MS)).await;
                continue;
            }

            // Strong diff: buy team+draw when sum < threshold
            let (team_token, team_price, team_outcome) = if sum_home_draw >= sum_away_draw {
                (&m.home_market.yes_token_id, ask_home, SoccerOutcome::HomeWin)
            } else {
                (&m.away_market.yes_token_id, ask_away, SoccerOutcome::AwayWin)
            };

            let min_price = MIN_ORDER_USD / shares_f;
            if team_price < min_price || ask_draw < min_price {
                info!("Skipping: leg price {:.4} < min {:.4} (min $1 order)", team_price.min(ask_draw), min_price);
                sleep(Duration::from_millis(PRICE_POLL_MS)).await;
                continue;
            }

            if simulation {
                info!(
                    "[SIM] Would buy: team @ {:.4} + draw @ {:.4} ({} shares)",
                    team_price, ask_draw, shares_f
                );
                holding = Some(HoldingPosition {
                    match_slug: m.slug.clone(),
                    team_token_id: team_token.to_string(),
                    team_outcome,
                    draw_token_id: Some(m.draw_market.yes_token_id.clone()),
                    buy_price_team: team_price,
                    buy_price_draw: ask_draw,
                    size: shares_f,
                });
                *last_trade_at.write().await = Some(std::time::Instant::now());
                sleep(Duration::from_millis(PRICE_POLL_MS)).await;
                continue;
            }

            // Place both buy orders (team + draw)
            let (_r1, _r2) = tokio::join!(
                api.place_market_order(team_token, shares_f, "BUY", Some("FAK")),
                api.place_market_order(&m.draw_market.yes_token_id, shares_f, "BUY", Some("FAK")),
            );
            info!("Bought team + draw for {}", m.title);
            holding = Some(HoldingPosition {
                match_slug: m.slug.clone(),
                team_token_id: team_token.to_string(),
                team_outcome,
                draw_token_id: Some(m.draw_market.yes_token_id.clone()),
                buy_price_team: team_price,
                buy_price_draw: ask_draw,
                size: shares_f,
            });
            *last_trade_at.write().await = Some(std::time::Instant::now());

            sleep(Duration::from_millis(PRICE_POLL_MS)).await;
        }

        Ok(())
    }

    async fn run_match_cycle(
        &self,
        m: SoccerMatch,
        last_trade_at: Arc<RwLock<Option<std::time::Instant>>>,
        initial_holding: Option<HoldingPosition>,
    ) -> Result<()> {
        Self::run_match_cycle_static(
            self.api.clone(),
            self.config.clone(),
            m,
            last_trade_at,
            initial_holding,
        ).await
    }
}
