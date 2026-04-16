# PROJECT_CONTEXT.md
# Polybot — BTC 15-min Spread Arbitrage

## What This Is
Arbitrage bot on Polymarket: buys Up + Down tokens on BTC 15-minute binary markets when the combined ask price is below breakeven. One token always resolves to $1.00, the other $0.00. Profit is locked in at entry if sum < `1/(1+2*fee)`.

## Entry Formula
```
breakeven  = 1 / (1 + 2 * taker_fee)   # fee=2% → 0.9804
threshold  = breakeven - min_margin     # margin=0.005 → 0.9754
entry when: up_ask + down_ask < threshold
profit/share = 1.0 - entry_sum
```

## Repo
- GitHub: `KirilliX/polybot`
- Default branch: `main`
- Local working directory: `/home/user/memory`

## Components
| Component | Path | Status |
|-----------|------|--------|
| React UI dashboard | `ui/` | Deployed, live |
| Node.js proxy server | `ui/server.js` | Running on server |
| Rust arb bot | `polymarket-arb-bot/` | Code complete, not yet deployed |
| GitHub Actions CI/CD | `.github/workflows/deploy.yml` | Active (deploys UI only) |

## Key URLs
- Live UI: `http://168.144.85.142:3000`
- Gamma API: `https://gamma-api.polymarket.com`
- CLOB API: `https://clob.polymarket.com`
- Data API: `https://data-api.polymarket.com`

## Market Pattern
Slug: `btc-updown-15m-{unix_timestamp}` — new market every 15 minutes. Markets may gap between expiry and next creation.

## Git Push
```bash
git push polybot main   # uses PAT remote named 'polybot'
# DO NOT use 'origin' — points to non-existent KirilliX/memory
```
