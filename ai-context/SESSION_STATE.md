# SESSION_STATE.md
# Current State (update after each meaningful task)

## Last Updated
2026-04-16

## What's Deployed
- **UI:** Live at `http://168.144.85.142:3000` — React dashboard with live CLOB prices
- **Bot:** Code complete in `polymarket-arb-bot/` but NOT running on server (no Docker files yet)

## Last Commits (main branch)
```
b00d751  docs: add CLAUDE.md and STACK.md
295111e  refactor: rewrite bot strategy from soccer to BTC 15-min spread arb
23922d4  feat: live CLOB prices for BTC 15-min market
```

## Known Active Issues
1. **UI market gap error:** "Нет активных BTC 15-min рынков" when no future markets exist
   - Root cause: `endDate > now` filter too strict; gap exists between 15-min cycles
   - Fix needed in `ui/src/App.jsx:27` — add fallback to most recently expired market

## Immediate Next Steps
1. Fix UI market gap bug → push → CI auto-deploys
2. Create Dockerfile + docker-compose.yml for bot
3. Deploy bot to server in simulation mode

## Environment
- Working dir: `/home/user/memory` (local clone of KirilliX/polybot)
- Push: `git push polybot main` (PAT remote configured)
- Server: `168.144.85.142`, user `root`, path `/root/polybot`
- Bot config on server: `/root/polybot-config/config.json` (not in repo, doesn't exist yet)

## Files Modified in Last Session
- `polymarket-arb-bot/src/btc.rs` (new)
- `polymarket-arb-bot/src/strategy.rs` (rewritten)
- `polymarket-arb-bot/src/config.rs` (simplified)
- `polymarket-arb-bot/src/main.rs` (soccer → btc)
- `polymarket-arb-bot/config.example.json` (updated)
- `ui/src/App.jsx` (live CLOB prices)
- `ui/server.js` (added /api/clob proxy)
- `CLAUDE.md` (project context for Claude Code)
- `STACK.md` (tech stack summary)
