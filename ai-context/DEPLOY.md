# DEPLOY.md

## UI Deployment (Automated)

**Trigger:** `git push polybot main`  
**Workflow:** `.github/workflows/deploy.yml`

Steps executed on server via SSH:
```bash
cd /root/polybot && git pull origin main
cd /root/polybot/ui
npm install --prefer-offline
npm run build
# Writes systemd unit, reloads daemon, restarts service
systemctl restart polybot-ui
```

Service config written by workflow:
```ini
[Unit]
Description=Polymarket Bot UI
After=network.target

[Service]
Type=simple
WorkingDirectory=/root/polybot/ui
ExecStart={node_bin} server.js
Restart=always
RestartSec=3
Environment=NODE_ENV=production
```

### Manual UI Deploy
```bash
# On server
cd /root/polybot && git pull origin main
cd ui && npm install && npm run build
systemctl restart polybot-ui
# Check: curl http://localhost:3000
```

---

## Bot Deployment (NOT YET IMPLEMENTED)

### Plan (to be implemented)
1. Create `polymarket-arb-bot/Dockerfile` (multi-stage Rust build)
2. Create `docker-compose.yml` in repo root
3. On server: create `/root/polybot-config/config.json` (not in repo)
4. Run: `docker compose up -d --build`
5. Logs: `docker logs -f polybot`

### Planned docker-compose.yml structure
```yaml
services:
  polybot:
    build: { context: ./polymarket-arb-bot, dockerfile: Dockerfile }
    container_name: polybot
    restart: unless-stopped
    volumes: [/root/polybot-config:/app/config:ro]
    environment: [RUST_LOG=info]
```

### Config on Server
Create `/root/polybot-config/config.json` (never commit to repo):
```json
{
  "polymarket": { "private_key": "...", "proxy_wallet_address": "0x...", "signature_type": 2 },
  "strategy": { "taker_fee": 0.02, "min_margin": 0.005, "shares": "100", "simulation_mode": true }
}
```
Start with `simulation_mode: true`, verify logs, then set `false`.

---

## Git Push Flow
```bash
# Always use polybot remote (has PAT embedded)
git push polybot main

# DO NOT use: git push origin main
# 'origin' → KirilliX/memory (non-existent repo)
```

## Secrets (GitHub)
| Secret | Value |
|--------|-------|
| `SERVER_HOST` | `168.144.85.142` |
| `SERVER_USER` | `root` |
| `SSH_PRIVATE_KEY` | SSH private key for server |
