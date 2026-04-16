# DECISIONS.md
# Key Architectural Decisions

## 2024 — Node.js Proxy Server Instead of Direct API Calls
**Decision:** Run `ui/server.js` as both static file server and CORS proxy.  
**Reason:** `gamma-api.polymarket.com` and `clob.polymarket.com` block direct browser requests (CORS). Proxy runs same-origin on port 3000.  
**Alternative rejected:** nginx reverse proxy — adds infrastructure complexity.

## 2024 — Two-Layer Price Architecture in UI
**Decision:** Gamma API every 60s (market discovery, token IDs) + CLOB API every 5s (live prices).  
**Reason:** Gamma API is slow and doesn't update prices in real-time. CLOB `/price` endpoint gives true live best-ask.  
**Note:** `clob_token_ids[0]` from Gamma is the "Yes" token ID used for price polling.

## 2024 — BTC 15-min Spread Arb Instead of Soccer Strategy
**Decision:** Rewrote bot strategy from soccer (3-way: home/draw/away) to BTC binary (Up/Down).  
**Reason:** BTC 15-min markets are pure binary — one leg always resolves $1.00. Risk-free profit if entry sum < breakeven. Soccer required complex risk management (live scores, team momentum).  
**Files removed from compilation:** `soccer.rs` (file kept but `mod soccer` removed from `main.rs`).

## 2024 — Gamma API Client-Side Filtering
**Decision:** Fetch `limit=200` events without server-side slug filter, filter client-side for `btc-updown-15m`.  
**Reason:** Gamma API `slug_contains` parameter is silently ignored — returns unrelated events.  
**Verified by:** UI was showing Macron election market instead of BTC market.

## 2024 — `endDate > now` Filter in UI (Soft)
**Current issue:** When no future markets exist (gap between 15-min cycles), UI shows error "Нет активных BTC 15-min рынков".  
**Status:** Known bug, not yet fixed. Fix: fall back to most recently expired market.

## 2024 — systemd Instead of nohup/pm2 for UI
**Decision:** systemd service `polybot-ui` for persistent Node.js process.  
**Reason:** `serve` binary path was unreliable (`/usr/bin` vs `/usr/local/bin`). systemd with `NODE_BIN=$(which node)` is portable.

## 2024 — YAML printf Instead of Heredoc for systemd Unit
**Decision:** Use `printf '...'` in deploy.yml to write systemd unit file.  
**Reason:** YAML parser interpreted `[Unit]` at column 0 in heredoc as YAML array syntax, causing parse error on line 50.

## 2024 — simulation_mode Default = true
**Decision:** Bot always defaults to simulation mode.  
**Reason:** Prevents accidental real orders during testing. Must explicitly set `"simulation_mode": false` in config.
