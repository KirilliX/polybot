# AGENT_RULES.md
# Rules for All Coding Agents

## Mandatory: Read These First
Before starting any work in this repo, always read:
1. `/ai-context/PROJECT_CONTEXT.md` — what this project is and does
2. `/ai-context/AGENT_RULES.md` — this file (working constraints)
3. `/ai-context/SESSION_STATE.md` — what's deployed, last changes, active bugs

Then read as needed:
- `/ai-context/ARCHITECTURE.md` — module structure, data flows
- `/ai-context/TASKS.md` — prioritized work queue
- `/ai-context/DECISIONS.md` — why things are the way they are

---

## Git Rules
- Push with: `git push polybot main`
- **Never** use `git push origin` — `origin` points to non-existent `KirilliX/memory`
- Never push to a branch other than `main` without explicit instruction
- Never force-push
- Never skip pre-commit hooks (`--no-verify`)

## Code Rules
- Do not refactor beyond what was asked
- Do not add error handling for impossible cases
- Do not add comments unless logic is non-obvious
- Do not create new files unless necessary
- `soccer.rs` is legacy dead code — do not delete without instruction, do not reference it

## Change Scope Rules
- Do not modify `.github/workflows/deploy.yml` unless explicitly asked
- Do not change secrets handling or infra configuration
- Do not modify `api.rs` authentication logic unless explicitly asked
- UI changes go in `ui/src/App.jsx` — keep all logic in one component

## Bot Safety Rules
- `simulation_mode` defaults to `true` — never set to `false` in committed code
- `config.json` (real credentials) is never committed — lives only on server at `/root/polybot-config/config.json`
- `config.example.json` uses placeholder strings, not real values

## After Each Meaningful Task
Update the following files as relevant:
- **Always:** `SESSION_STATE.md` — update last commits, active issues, next steps
- **If architecture changed:** `ARCHITECTURE.md`, `DECISIONS.md`
- **If deployment changed:** `DEPLOY.md`
- **If task status changed:** `TASKS.md`

## Known Gotchas
1. Gamma API `slug_contains` param is silently ignored — always fetch `limit=200` and filter client-side
2. CLOB `clobTokenIds[0]` is the "Yes" token; `[1]` is "No" — always use `[0]`
3. BTC 15-min markets can have gaps between expiry and next creation — UI must handle this gracefully
4. YAML `[Unit]` at column 0 breaks GitHub Actions YAML parser — use `printf` for systemd unit files
5. `polymarket-client-sdk` re-authenticates on every `place_market_order` call — expected, not a bug
