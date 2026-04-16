# TASKS.md
# Task Status

## Done
- [x] React UI with live CLOB prices (Gamma 60s + CLOB 5s)
- [x] Node.js CORS proxy (`/api/gamma`, `/api/clob`)
- [x] systemd service `polybot-ui` (port 3000)
- [x] GitHub Actions CI/CD (push to main → SSH → npm build → systemctl restart)
- [x] Rust bot rewritten: soccer → BTC 15-min spread arb
- [x] `btc.rs`: market discovery for `btc-updown-15m-*`
- [x] `strategy.rs`: BtcArbStrategy (poll prices, entry at sum < threshold)
- [x] `config.rs`: simplified to BTC arb params (taker_fee, min_margin, etc.)
- [x] `config.example.json`: updated for BTC arb

## Active Bugs
- [ ] UI shows "Нет активных BTC 15-min рынков" during market gap (endDate filter too strict)
  - Fix: if no future markets, fall back to most recently expired one
  - File: `ui/src/App.jsx` `fetchBtcMarketInfo()`

## Next (Prioritized)
1. Fix market gap bug in UI (`fetchBtcMarketInfo` fallback)
2. Create `polymarket-arb-bot/Dockerfile` (multi-stage Rust build)
3. Create `docker-compose.yml` in repo root
4. Add bot deploy step to `.github/workflows/deploy.yml`
5. Create `/root/polybot-config/config.json` on server
6. Test bot in simulation mode on server
7. Switch `simulation_mode: false` after verification

## Future
- Bot: add `/status` HTTP endpoint (port 8080) for UI to query bot state
- UI: show bot status (running / last entry / last PnL)
- UI: start/stop bot from dashboard
- Bot: dynamic order sizing based on order book depth
