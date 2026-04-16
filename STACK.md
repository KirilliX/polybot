# Polybot — Tech Stack

## UI
- **React 18** + Vite + Tailwind CSS
- **Recharts** — waterfall chart
- **Node.js** — `ui/server.js` (static + CORS proxy на порту 3000)
- Proxy: `/api/gamma` → `gamma-api.polymarket.com`, `/api/clob` → `clob.polymarket.com`

## Bot
- **Rust** (tokio async, reqwest, serde)
- **polymarket-client-sdk** — order signing (CLOB)
- **alloy** — Polygon RPC, EVM tx (redeem)
- **Docker** — multi-stage build, config mounted из `/root/polybot-config/config.json`

## APIs
| API | Назначение | Интервал |
|-----|-----------|---------|
| `gamma-api.polymarket.com/events` | Поиск рынков, token IDs | 60с |
| `clob.polymarket.com/price` | Live best-ask | 5с |
| `clob.polymarket.com/book` | Order book (bot) | 5с |
| `data-api.polymarket.com/positions` | Позиции кошелька | по запросу |

## Infra
- **Server:** `168.144.85.142` (Ubuntu)
- **systemd** — `polybot-ui` (Node.js)
- **Docker Compose** — бот
- **GitHub Actions** — CI/CD (push to `main` → SSH deploy)
- **Repo:** `KirilliX/polybot` (push через remote `polybot` с PAT)
