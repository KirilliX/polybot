# ARCHITECTURE.md

## UI — `ui/`

### Files
```
ui/
├── src/
│   ├── App.jsx        # Main component — all logic lives here
│   └── main.jsx       # React entry point
├── server.js          # Node.js HTTP server (static + CORS proxy)
├── package.json       # React 18, Recharts, Lucide, Vite, Tailwind
├── vite.config.js
├── tailwind.config.js
└── dist/              # Built output (gitignored, built on server)
```

### Data Flow
```
Browser
  └─ GET /api/gamma/events?limit=200  (every 60s)
        → server.js proxy → gamma-api.polymarket.com
        → App.jsx: filter slug contains 'btc-updown-15m' + endDate > now
        → extract up_token_id, down_token_id from clobTokenIds[0]
  └─ GET /api/clob/price?token_id={id}&side=buy  (every 5s, per token)
        → server.js proxy → clob.polymarket.com
        → App.jsx: setMarket({ yesPrice, noPrice })
        → pipeline calc → waterfall chart
```

### App.jsx State
| State | Type | Purpose |
|-------|------|---------|
| `config` | object | takerFee, threshold, bookDepth, targetVolume |
| `market` | `{yesPrice, noPrice}` | Live Up/Down ask prices |
| `marketInfo` | `{title, endTime, slug}` | Current market metadata |
| `tokenIds` | `{up, down}` | CLOB token IDs for price polling |
| `ttl` | number (ms) | Countdown to market expiry |
| `liveStatus` | `'loading'|'live'|'error'` | Connection state |

### server.js Routes
```
/api/gamma/*  →  https://gamma-api.polymarket.com/*
/api/clob/*   →  https://clob.polymarket.com/*
/*            →  dist/ (SPA fallback to index.html)
```
Port: 3000

---

## Bot — `polymarket-arb-bot/`

### Files
```
polymarket-arb-bot/
├── src/
│   ├── main.rs        # Entry point, CLI args (--config, --redeem, --condition-id)
│   ├── btc.rs         # BtcDiscovery: fetch active btc-updown-15m markets
│   ├── strategy.rs    # BtcArbStrategy: poll prices, entry logic, order placement
│   ├── config.rs      # Config/StrategyConfig structs, load from JSON
│   ├── api.rs         # PolymarketApi: authenticate, get_best_ask, place_market_order, redeem_tokens
│   └── models.rs      # OrderRequest, OrderResponse, Position, BalanceAllowance
├── Cargo.toml
├── config.example.json
└── soccer.rs          # UNUSED — legacy soccer strategy (not compiled)
```

### Bot Data Flow
```
main.rs
  → Config::load(config.json)
  → PolymarketApi::authenticate() (if private_key present)
  → BtcArbStrategy::run() loop:
      → BtcDiscovery::fetch_active_markets()
            GET gamma-api.polymarket.com/events?limit=200
            filter: slug contains 'btc-updown-15m', endDate > now
            sort by endDate asc
      → pick market with TTL > min_seconds_before_expiry
      → run_market_cycle() loop (every price_poll_interval_ms):
            tokio::join!(api.get_best_ask(up_token), api.get_best_ask(down_token))
            GET clob.polymarket.com/book?token_id={id}
            if sum < threshold: tokio::join!(place_market_order×2)
```

### Key Crate Versions
| Crate | Version | Role |
|-------|---------|------|
| `tokio` | 1.35 | Async runtime |
| `reqwest` | 0.11 | HTTP client |
| `polymarket-client-sdk` | 0.4.2 | Order signing (CLOB) |
| `alloy` | 1.3 | Polygon RPC / EVM tx (redeem) |
| `clap` | 4.4 | CLI args |

### Config Schema (config.json)
```json
{
  "polymarket": {
    "gamma_api_url": "https://gamma-api.polymarket.com",
    "clob_api_url": "https://clob.polymarket.com",
    "api_key": "...",
    "api_secret": "...",
    "api_passphrase": "...",
    "private_key": "hex",
    "proxy_wallet_address": "0x...",
    "signature_type": 2,
    "rpc_url": "https://polygon-rpc.com"
  },
  "strategy": {
    "taker_fee": 0.02,
    "min_margin": 0.005,
    "shares": "100",
    "price_poll_interval_ms": 5000,
    "market_refresh_interval_secs": 60,
    "min_seconds_before_expiry": 120,
    "simulation_mode": true
  }
}
```

---

## Infra

### Server
- Host: `168.144.85.142` (Ubuntu)
- UI: systemd service `polybot-ui` → `node /root/polybot/ui/server.js`
- Bot: **NOT YET DEPLOYED** — Docker Compose planned, files not created

### CI/CD
- Trigger: push to `main` or `workflow_dispatch`
- Action: `appleboy/ssh-action@v1.0.3`
- Deploys: UI only (`npm install`, `npm run build`, `systemctl restart polybot-ui`)
- Bot deploy: not in workflow yet
- Secrets: `SERVER_HOST`, `SERVER_USER`, `SSH_PRIVATE_KEY`
