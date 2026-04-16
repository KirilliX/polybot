# Polybot — BTC 15-min Spread Arbitrage

## Проект
Арбитражный бот на Polymarket: покупает Up+Down на BTC 15-минутных рынках когда сумма цен < breakeven.

**Репо:** `KirilliX/polybot` · **Сервер:** `168.144.85.142` · **UI:** `http://168.144.85.142:3000`

## Git
- **Рабочая ветка:** `main` (push через `git push polybot main`)
- **Remote:** `polybot` → `https://ghp_...@github.com/KirilliX/polybot.git` (PAT уже настроен)
- **Локальный remote `origin`** указывает на несуществующий `KirilliX/memory` — не использовать

## Архитектура

### UI (`ui/`)
- `src/App.jsx` — React: Gamma API (60с) → token IDs → CLOB API (5с) → live цены Up/Down
- `server.js` — Node.js прокси: `/api/gamma` → `gamma-api.polymarket.com`, `/api/clob` → `clob.polymarket.com`
- Деплой: `npm run build` → systemd `polybot-ui` (`node server.js`, порт 3000)
- CI/CD: `.github/workflows/deploy.yml` (push to main → SSH deploy)

### Бот (`polymarket-arb-bot/`)
- **Язык:** Rust · **Бинарник:** `polymarket-arbitrage-bot`
- `src/btc.rs` — поиск `btc-updown-15m-*` рынков из Gamma API
- `src/strategy.rs` — `BtcArbStrategy`: poll Up+Down ask, entry когда `sum < 1/(1+2*fee) - margin`
- `src/api.rs` — CLOB auth, `get_best_ask`, `place_market_order`, `redeem_tokens`
- `src/config.rs` — `taker_fee`, `min_margin`, `shares`, `simulation_mode`
- `config.example.json` — шаблон конфига

## Формула входа
```
breakeven = 1 / (1 + 2 * taker_fee)   # при fee=2%: ~0.9804
threshold = breakeven - min_margin      # при margin=0.005: ~0.9754
entry: up_ask + down_ask < threshold
profit_per_share = 1.0 - entry_sum      # одна нога всегда = $1.00
```

## Конфиг бота (`config.json` — не в репо, только на сервере)
```json
{
  "polymarket": { "private_key": "...", "proxy_wallet_address": "0x...", "signature_type": 2 },
  "strategy": { "taker_fee": 0.02, "min_margin": 0.005, "shares": "100", "simulation_mode": true }
}
```

## Деплой бота
- `docker-compose.yml` в корне: монтирует `/root/polybot-config/config.json`
- Запуск: `docker compose up -d --build`
- Логи: `docker logs -f polybot`

## Текущий статус
- [x] UI с live CLOB ценами задеплоен
- [x] Rust-бот переписан на BTC spread арб
- [ ] Бот задеплоен на сервер через Docker
- [ ] `config.json` на сервере с реальными ключами
- [ ] Тест в simulation_mode на сервере

## Следующие шаги
1. SSH на сервер, создать `/root/polybot-config/config.json`
2. `docker compose up -d --build` (или добавить в deploy.yml)
3. Проверить логи в sim-режиме
4. Переключить `simulation_mode: false`
