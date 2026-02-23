# Kairex — Development Roadmap

Phased implementation plan from empty repo to production launch. Each phase produces something testable before moving to the next. Testing and CI are woven in from the start — not a phase, a constant.

---

## Phase 0 — Infrastructure Foundation

Everything the application needs to exist before a line of Rust is written.

### 0.1 — Repository Structure

Set up the project skeleton that everything else builds on.

**Deliverables:**
- Rust workspace initialized (`cargo init`, workspace Cargo.toml)
- Python scripts directory with `manifest.toml`, empty entry points, `requirements.txt`
- Config directory with starter TOML files (assets, schedules, thresholds, collection, free_channel)
- Prompts directory with placeholder identity.md, system.md, schema JSONs
- Test fixtures directory structure (`tests/fixtures/binance/`, `python/`, `llm/`, `config/`)
- Dockerfile (multi-stage: Rust build → runtime with Python + deps)
- `docker-compose.yml` (app + full observability stack)
- `docker-compose.dev.yml` (local dev overrides — no observability stack, mounts source)
- `.github/workflows/ci.yml` — initial CI pipeline (fmt, clippy, test, pytest)
- `.gitignore`, `.dockerignore`

**Exit criteria:** `cargo build` succeeds. `cargo test` runs (zero tests, zero failures). CI pipeline passes on push. Docker image builds. `docker compose up` starts all services (app exits immediately — no logic yet, but observability stack is reachable).

### 0.2 — Ansible Playbooks

VPS management infrastructure. All playbooks written and tested against a fresh VPS before any application deployment.

**Deliverables:**

**`playbooks/harden.yml`** — VPS hardening:
- Create `deploy` user with SSH key auth
- Add deploy user's public key from local machine
- Configure passwordless sudo for Docker commands only (`/usr/bin/docker`, `/usr/bin/docker compose`)
- Disable password authentication in sshd_config
- Disable root SSH login — but ONLY after verifying deploy user can connect (handler with validation step)
- Install and configure ufw (allow SSH 22/tcp, WireGuard 51820/udp, deny all other inbound)
- Install and configure fail2ban (SSH jail)
- Enable unattended-upgrades for security patches
- Set timezone to UTC

**`playbooks/wireguard.yml`** — WireGuard VPN server:
- Install WireGuard
- Generate server keypair (idempotent — skip if keys exist)
- Deploy wg0.conf with NAT masquerade rules
- Open firewall port (51820/udp — already in harden, but idempotent)
- Enable and start wg-quick@wg0
- Output server public key for client configuration

**`playbooks/teardown-wireguard.yml`** — clean removal:
- Stop and disable wg-quick@wg0
- Remove WireGuard config and keys
- Remove firewall rule
- Uninstall WireGuard package

**`playbooks/docker.yml`** — Docker installation:
- Install Docker Engine + Compose plugin from official Docker repo
- Add deploy user to docker group
- Configure Docker logging driver (json-file with max-size rotation)
- Verify `docker compose version` works as deploy user

**`playbooks/deploy.yml`** — application deployment:
- Sync project files to VPS (rsync or git pull — decide during implementation)
- Decrypt and deploy `.env` from Ansible Vault
- Run `docker compose pull` / `docker compose build`
- Run `docker compose up -d`
- Health check: verify app container is running, Grafana is reachable, Prometheus is scraping
- Output deployment status summary

**Supporting files:**
- `ansible/inventory/production.ini`
- `ansible/vars/secrets.yml` (Ansible Vault encrypted — Anthropic key, Telegram tokens, channel IDs)
- `ansible/ansible.cfg`
- `ansible/roles/` structure with tasks, templates, handlers per role

**Exit criteria:** Run all four playbooks against a fresh Debian 13 VPS in sequence. Deploy user can SSH in. Root cannot. Firewall active. WireGuard tunnel established from dev machine. Docker running. `docker compose up` on VPS starts the full stack.

### 0.3 — Dev Environment

Local development workflow that mirrors production.

**Deliverables:**
- `docker-compose.dev.yml` — overrides for local dev (mount source code, no observability stack, expose debug ports)
- WireGuard client configured on dev machine per `docs/DEV_VPN.md`
- Binance API connectivity verified through VPN tunnel
- `Makefile` or `justfile` with common commands: `build`, `test`, `dev`, `deploy`, `lint`
- Pre-commit hooks: `cargo fmt`, `cargo clippy`

**Exit criteria:** `make dev` starts the local container. Binance REST API calls succeed through the VPN. `make test` runs full test suite. `make deploy` triggers the Ansible deploy playbook.

---

## Phase 1 — Storage Layer

The foundation everything else reads from and writes to.

### 1.1 — SQLite Schema and Connection Management

**Deliverables:**
- Schema migration system (embedded SQL files applied on startup, version tracked)
- Initial migration: all tables from DESIGN.md (candles, funding_rates, open_interest, indices, system_outputs, active_setups, fired_alerts)
- WAL mode enabled on database creation
- Connection management: write connection (`Arc<Mutex<Connection>>`), read connection pool
- Database module with typed query functions — not raw SQL scattered through the codebase

**Tests:**
- Create in-memory database, apply migrations, verify all tables exist with correct columns
- Insert and query each table type (candles, funding rates, OI, indices)
- Verify primary key constraints (duplicate insert rejected)
- Verify WAL mode is active

**Exit criteria:** Database module compiles, all tests pass, CI green.

### 1.2 — System Output Storage

**Deliverables:**
- Insert system output (report type, schema version, JSON blob)
- Query system outputs by type, date range, most recent
- Setup extraction: given a system output JSON, extract setups into `active_setups`
- Setup superseding: new report's setups mark previous setups as superseded/expired
- Transactional insert (system output + setup extraction in one transaction)

**Tests:**
- Insert a morning report, verify setups extracted correctly
- Insert a midday report, verify morning setups for overlapping assets superseded
- Insert a report with no setups for an asset, verify previous setup expired
- Query today's reports by type, verify ordering and completeness
- Verify transaction atomicity (partial failure rolls back both)

**Exit criteria:** Full system output lifecycle tested. Setup superseding works correctly. CI green.

### 1.3 — Tiered Retention

**Deliverables:**
- Aggregation functions: 5m → 1h, 1h → 1d (OHLCV merge logic)
- Pruning functions: delete candles older than retention policy
- Background job that runs aggregation then pruning (sequence enforced)
- Configurable retention periods from `config/collection.toml`

**Tests:**
- Insert 12 consecutive 5m candles, aggregate to 1h, verify OHLCV correct (high = max of highs, low = min of lows, etc.)
- Insert candles spanning retention boundary, run pruning, verify old ones gone and new ones preserved
- Verify aggregation → prune ordering (never lose data)

**Exit criteria:** Retention lifecycle works correctly. CI green.

---

## Phase 2 — Collection Layer

Data flowing into storage from external sources.

### 2.1 — Binance REST Client

Start with REST because it's simpler and needed for backfill regardless.

**Deliverables:**
- HTTP client module wrapping Binance REST endpoints
- Candle fetch: `/api/v3/klines` with symbol, interval, startTime, endTime, pagination
- Funding rate fetch: `/fapi/v1/fundingRate`
- Open interest fetch: `/fapi/v1/openInterest`
- Rate limiting awareness (Binance weight limits)
- Response deserialization into storage-ready types

**Tests:**
- Deserialize fixture responses (saved from real API calls) into Rust types
- Verify pagination logic with multi-page fixture
- Verify rate limit weight tracking

**Manual verification:** Make real API calls through VPN, save responses as test fixtures.

**Exit criteria:** Client compiles and passes fixture tests. Real API calls verified manually. CI green.

### 2.2 — External Data Clients

**Deliverables:**
- Alternative.me Fear & Greed client
- CoinGecko dominance/market cap client (with API key header)
- Response deserialization into storage-ready types

**Tests:**
- Fixture-based deserialization tests for each API

**Manual verification:** Real API calls, save fixture responses.

**Exit criteria:** All external clients compile and pass tests. CI green.

### 2.3 — Gap Detection and Backfill

**Deliverables:**
- Per-collector gap detection: check most recent timestamp in storage, compare to current time
- Backfill orchestration: fetch historical data with time range parameters, write to storage
- Pagination handling for large backfills (cold start: potentially thousands of candles)
- Gap detection on startup and on-demand (callable after reconnection)

**Tests:**
- Empty database → gap detection returns full history range
- Database with data up to 2 hours ago → gap detection returns 2-hour range
- Backfill writes correct data to storage (verify with storage queries)

**Exit criteria:** Gap detection and backfill work correctly against fixtures. CI green.

### 2.4 — Binance WebSocket Client

**Deliverables:**
- Combined stream connection for all assets × timeframes
- Message deserialization (kline events with `is_final` flag)
- Final candle → storage write
- Automatic reconnection with exponential backoff
- Gap detection on reconnect
- Event emission via `tokio::broadcast` channel

**Tests:**
- Deserialize fixture WebSocket messages
- Verify only final candles trigger storage writes
- Verify reconnection logic (mock connection drop)
- Verify event emission on new candle

**Manual verification:** Connect to real Binance WebSocket through VPN, observe candles flowing into storage.

**Exit criteria:** WebSocket client runs continuously, stores candles, reconnects on drop. CI green.

### 2.5 — REST Polling Loops

**Deliverables:**
- Independent polling tasks per data type (funding rates 8h, OI hourly, Fear & Greed daily, dominance daily)
- Retry policy: backoff on failure, skip after 3 consecutive failures, log prominently
- Config-driven intervals from `config/collection.toml`

**Tests:**
- Verify polling respects configured intervals
- Verify retry/skip logic

**Manual verification:** Run collectors, observe data accumulating in storage across all types.

**Exit criteria:** All collectors running, all data types flowing into storage. CI green.

### Checkpoint: Data Pipeline Verified

At this point the system has a complete data pipeline: WebSocket + REST → Storage with gap detection, backfill, and retention. Verify by running the system for 24+ hours and confirming:
- All asset candles present at all timeframes
- Funding rates arriving every 8h
- Open interest updating hourly
- Fear & Greed and dominance updating daily
- No gaps in data
- Retention aggregation working on schedule

---

## Phase 3 — Python Analysis Engine

The math layer.

### 3.1 — Subprocess Infrastructure

**Deliverables:**
- Rust module for Python subprocess management: spawn, pipe JSON to stdin, read stdout, enforce timeout
- Manifest parser: read `scripts/manifest.toml`, extract data requirements per script
- Data serialization: query storage per manifest requirements, serialize to JSON input shape
- Output deserialization: parse Python's JSON output into Rust types
- Error handling: timeout, non-zero exit, malformed JSON, stderr capture

**Tests:**
- Serialize sample storage data to JSON, verify shape matches contract
- Deserialize fixture Python output, verify parsing
- Verify timeout enforcement (mock slow subprocess)
- Verify error handling on bad exit code and malformed output

**Exit criteria:** Subprocess infrastructure compiles and passes all tests. CI green.

### 3.2 — Indicator Computation Scripts

**Deliverables:**
- `scripts/compute_indicators.py` — computes all indicators for all assets
- `scripts/lib/indicators.py` — RSI, Bollinger bands/bandwidth, MACD, ADX, EMA ribbon, volume ratios
- Each indicator verified against known values
- Output shape: flat key-value per asset matching the contract

**Tests (pytest):**
- Each indicator function tested with known input/output pairs
- Full `compute_indicators.py` tested end-to-end with fixture input → verify output shape and values
- Edge cases: insufficient data (fewer candles than indicator period), all-zero volume, single candle

**Exit criteria:** All indicators compute correctly. End-to-end subprocess test passes (Rust calls Python with fixture data, gets valid output). CI green.

### 3.3 — Context Building Scripts

**Deliverables:**
- `scripts/build_context.py` — assembles the full numerical context for LLM reports
- `scripts/lib/context.py` — context assembly helpers
- Per-report-type context structure (morning gets different framing than evening)
- Includes computed indicators + raw data summaries + cross-market context

**Tests (pytest):**
- Full context build with fixture data, verify output structure
- Each report type produces appropriate context

**Exit criteria:** Context building works end-to-end. Rust calls Python, gets valid context JSON. CI green.

### Checkpoint: Analysis Pipeline Verified

Run the full chain: storage (with real collected data) → Python subprocess → indicator output. Verify indicators compute correctly against live market data. Compare computed RSI/Bollinger/etc. against a known charting tool for sanity.

---

## Phase 4 — Live Evaluation Loop

The 5-minute heartbeat.

### 4.1 — Alert Condition Evaluation

**Deliverables:**
- Setup trigger evaluation: price_above, price_below, price_percent_move
- System rule evaluation: parse TOML conditions, evaluate against indicator output
- Alert deduplication: cooldown tracking per asset per alert type via `fired_alerts` table
- Overnight noise reduction: configurable threshold multiplier per time window

**Tests:**
- Price above/below triggers with various indicator values
- System rules: volume spike, RSI extreme, funding extreme
- Deduplication: same condition within cooldown → suppressed. After cooldown → fires again
- Overnight: same event that would fire during day is suppressed overnight (with higher threshold)

**Exit criteria:** All evaluation logic tested exhaustively. Pure logic, no external deps. CI green.

### 4.2 — The 5-Minute Cycle

**Deliverables:**
- Timer-driven loop: every 5 minutes (aligned to candle close)
- Queries storage for indicator computation data
- Calls Python subprocess
- Evaluates all active setups + system rules against results
- Emits trigger events for any fired conditions
- Full tracing spans on every step

**Tests:**
- Integration test: load fixtures into storage, mock Python subprocess, run one cycle, verify correct trigger decisions
- Verify cycle completes within reasonable time (< 30 seconds for all assets)

**Manual verification:** Run with live data and active setups, observe evaluation decisions in logs/traces.

**Exit criteria:** Live loop runs continuously, evaluates correctly, doesn't miss cycles. CI green.

---

## Phase 5 — LLM Integration

The reasoning engine.

### 5.1 — Anthropic API Client

**Deliverables:**
- HTTP client for Anthropic Messages API
- Tool use for structured output: define output schema as tool, force tool choice
- Response parsing: extract tool call arguments, deserialize into Rust structs via serde
- Retry logic: rate limit backoff, transient error retry
- Token usage tracking (for metrics)
- Full request/response logging to SQLite (LLM thought storage)

**Tests:**
- Deserialize fixture LLM responses into Rust structs for all 5 report types
- Verify schema version handling and forward compatibility (`#[serde(default)]`)
- Verify retry logic with mock error responses
- Verify thought storage writes complete request/response

**Manual verification:** Make real Anthropic API call with sample context and schema. Save response as fixture. Verify structured output quality.

**Exit criteria:** API client compiles, all fixture tests pass, thought storage working. CI green.

### 5.2 — Output Schemas

**Deliverables:**
- JSON schema definitions for all 5 report types (`prompts/schemas/*.json`)
- Corresponding Rust structs with serde derives
- Schema includes: regime fields, setup array, narrative fields, significance ratings, free_narrative fields
- Per-type variations: evening has scorecard fields, weekly has week narrative + "what would change my mind"
- Schema validation on deserialized output (required fields present, values in range)

**Tests:**
- Each schema validates against its fixture response
- Verify significance fields present on all types (magnitude, surprise, regime_relevance)
- Verify setup structs parse with all trigger condition types
- Verify free_narrative fields present where expected

**Exit criteria:** All schemas defined, all Rust types compile and deserialize fixtures correctly. CI green.

### 5.3 — Prompt Architecture

**Deliverables:**
- `prompts/identity.md` — the analyst's character (from DESIGN.md Identity section)
- `prompts/system.md` — system understanding, data semantics, output expectations
- Prompt assembly module: reads prompt files, combines with per-call context, constructs API request
- Per-report-type context assembly: morning gets overnight data + empty prior, midday gets morning reference, evening gets full day, weekly gets full week

**Tests:**
- Prompt assembly produces valid API request structure
- Verify per-report context includes correct prior outputs (mock storage queries)

**Manual verification:** Generate each report type with real data. Review output quality. Iterate on prompts until the voice and analytical depth match DESIGN.md examples.

**Exit criteria:** All 5 report types generate successfully with appropriate voice and quality. This is subjective — requires manual review and prompt iteration.

---

## Phase 6 — Report Generation Pipeline

Connecting everything into the scheduled production flow.

### 6.1 — Scheduler

**Deliverables:**
- Cron-style job scheduler (tokio-cron-scheduler or equivalent)
- Jobs for: morning (8:15 AM ET), midday (11:45 AM ET), evening (7:45 PM ET), weekly (Sunday afternoon — time TBD)
- Generation buffer: trigger before delivery time, hold until scheduled time
- Missed job detection: if trigger time passed without firing, generate immediately
- Configurable schedule from `config/schedules.toml`

**Tests:**
- Verify jobs fire at correct times (mock clock)
- Verify hold-until-delivery logic
- Verify missed job detection

**Exit criteria:** Scheduler fires reliably. CI green.

### 6.2 — Full Pipeline Integration

**Deliverables:**
- The complete 12-step pipeline wired together: scheduler → storage query → staleness check → Python → prompt assembly → LLM → validation → storage write + setup extraction → hold → delivery → free routing
- Error handling at each step per DESIGN.md specification
- Tracing spans on every step
- Metrics emission (generation duration, delivery delay, LLM latency)

**Tests:**
- Integration test: full pipeline with mocked LLM, verify complete flow end-to-end
- Error injection: mock failures at each step, verify correct handling (retry, abort, degrade)

**Manual verification (dry-run mode):** Run the full pipeline with real data and real LLM, outputting to logs instead of Telegram. Review report quality, timing, and traces.

**Exit criteria:** Full pipeline runs reliably in dry-run mode. All 3 daily reports + weekly generate correctly on schedule. Traces visible. Metrics emitting. CI green.

---

## Phase 7 — Delivery

Getting reports to users.

### 7.1 — Telegram Bot Client

**Deliverables:**
- Telegram Bot API client: send message, send formatted message (Markdown/HTML)
- Message formatting: render structured output narrative fields into Telegram-compatible format
- Message splitting: handle 4096 character limit (split at logical breakpoints if needed)
- Delivery status tracking: update `system_outputs.delivery_status` and `delivered_at`
- Retry with backoff on Telegram API failures

**Tests:**
- Format fixture reports into Telegram messages, verify output
- Verify message splitting on long content
- Verify delivery status updates in storage
- Verify retry logic

**Manual verification:** Send formatted reports to a test Telegram channel. Review formatting, readability, rendering.

**Exit criteria:** Reports post correctly to Telegram with clean formatting. CI green.

### 7.2 — Premium/Free Channel Routing

**Deliverables:**
- Routing module: evaluate significance ratings against `config/free_channel.toml` threshold rules
- Premium delivery: always post full report to premium channel
- Free channel: condensed evening recap (via `free_narrative` field), weekly scorecard pass-through, threshold-gated morning reports and alerts
- Upsell placement on free channel messages
- Configurable routing rules per event type

**Tests:**
- Routing evaluation with various significance scores and rule configs
- Verify always-route types bypass threshold check
- Verify threshold-gated types respect config
- Verify upsell text appended to free messages

**Exit criteria:** Dual-channel delivery working. Premium gets everything, free gets configured subset. CI green.

### 7.3 — Alert Delivery

**Deliverables:**
- Alert pipeline: trigger event from live evaluation → context bundle → LLM call → format → deliver
- Connects the live evaluation loop (Phase 4) to the LLM (Phase 5) to delivery (Phase 7.1)
- Alert also stores as system output and routes through free channel logic
- Overnight noise reduction applied before LLM call (don't generate alerts that won't be sent)

**Tests:**
- Integration test: mock trigger event → verify LLM called → verify message delivered
- Verify overnight suppression

**Manual verification:** With live data, trigger a condition and observe the full alert flow end-to-end.

**Exit criteria:** Alerts fire and deliver correctly. Full flow traceable in Grafana. CI green.

### Checkpoint: Full Product Verified in Dry-Run

The system is feature-complete. Running in dry-run mode against a test Telegram channel:
- Three daily reports generating and posting on schedule
- Alerts firing on live data and posting
- Weekly briefing generating on Sunday
- Free channel receiving configured subset
- All traces and metrics visible in Grafana
- Setup lifecycle working (morning creates, midday supersedes, evening scores)
- Accountability loop working (evening references morning calls)

Run for a full week in dry-run. Review every report. Iterate on prompts, thresholds, and formatting. This is where the product gets polished.

---

## Phase 8 — Observability Dashboards & Alerting

The observability infrastructure (Tempo, Prometheus, Loki, Grafana) is running since Phase 0. Instrumentation has been added throughout every phase. This phase builds the dashboards and alerting rules that make it usable.

### 8.1 — Grafana Dashboards

**Deliverables:**
- **System Overview** — collection status per source/symbol, live cycle health, report generation status, delivery success rate
- **LLM Performance** — call latency distribution, token usage per report type, error rate, cost tracking
- **Report Timeline** — scheduled vs actual delivery times, generation duration, any delays
- **Alert Activity** — alerts fired by type/asset, deduplication rate, overnight suppression rate
- **Data Health** — collection freshness per source, gap detection events, backfill activity
- All dashboards provisioned as code (Grafana provisioning JSON, version controlled)

**Exit criteria:** Dashboards provide complete visibility into system health and behavior.

### 8.2 — Grafana Alert Rules

**Deliverables:**
- Collection staleness: alert if any source hasn't updated in 2x its expected interval
- Report delivery failure: alert if a scheduled report fails to deliver
- LLM errors: alert on 3+ consecutive failures
- Python subprocess crash: alert on any non-zero exit
- Disk usage: alert if SQLite database approaching volume capacity
- All alerts route to operator Telegram channel via Grafana notification channel
- Alert rules provisioned as code

**Exit criteria:** Operator receives Telegram alerts for all critical system failures. No false positives during 48-hour test.

---

## Phase 9 — Production Launch

### 9.1 — Pre-Launch Checklist

- [ ] Full week of dry-run completed with acceptable report quality
- [ ] All prompts finalized and reviewed
- [ ] Asset universe finalized in config
- [ ] Significance thresholds tuned for free channel
- [ ] Overnight noise reduction thresholds calibrated
- [ ] All Grafana dashboards operational
- [ ] All Grafana alerts operational and tested
- [ ] Ansible deploy playbook tested (full teardown and redeploy)
- [ ] Backup strategy for SQLite database (automated snapshot)
- [ ] Telegram channels created (premium private, free public, operator alerts)
- [ ] Telegram Stars subscription configured on premium channel
- [ ] Dry-run mode toggle verified (easy switch between dry-run and live)

### 9.2 — Launch Sequence

1. Switch from test channels to production channels in config
2. Deploy via Ansible
3. Monitor first morning report generation in Grafana traces
4. Verify delivery to premium channel
5. Verify free channel routing
6. Monitor full first day (all 3 reports + any alerts)
7. Monitor first full week through Sunday weekly briefing

### 9.3 — Post-Launch

- Remove WireGuard from VPS (dev-phase infrastructure): `ansible-playbook playbooks/teardown-wireguard.yml`
- Monitor system health daily for first two weeks
- Iterate on prompts based on live output quality
- Tune significance thresholds based on free channel activity
- Tune alert thresholds based on alert volume and relevance

---

## Phase Dependencies

```
Phase 0 (Infrastructure)
  ├── 0.1 Repo Structure ─────────────────────────────┐
  ├── 0.2 Ansible ─────────────────────────────────────┤
  └── 0.3 Dev Environment (depends on 0.1, 0.2) ──────┤
                                                       │
Phase 1 (Storage) depends on 0.1 ─────────────────────┤
  ├── 1.1 Schema + Connections                         │
  ├── 1.2 System Output Storage (depends on 1.1)       │
  └── 1.3 Tiered Retention (depends on 1.1)            │
                                                       │
Phase 2 (Collection) depends on 1.1 ──────────────────┤
  ├── 2.1 Binance REST                                 │
  ├── 2.2 External Data Clients                        │
  ├── 2.3 Gap Detection (depends on 2.1)               │
  ├── 2.4 WebSocket (depends on 2.1, 2.3)              │
  └── 2.5 REST Polling (depends on 2.1, 2.2)           │
                                                       │
Phase 3 (Python) depends on 1.1 ──────────────────────┤
  ├── 3.1 Subprocess Infrastructure                    │
  ├── 3.2 Indicator Scripts (depends on 3.1)           │
  └── 3.3 Context Building (depends on 3.1)            │
                                                       │
Phase 4 (Live Eval) depends on 3.2, 1.2 ─────────────┤
  ├── 4.1 Alert Condition Evaluation                   │
  └── 4.2 5-Minute Cycle (depends on 4.1)              │
                                                       │
Phase 5 (LLM) depends on 3.3, 1.2 ───────────────────┤
  ├── 5.1 Anthropic Client                             │
  ├── 5.2 Output Schemas (depends on 5.1)              │
  └── 5.3 Prompt Architecture (depends on 5.1, 5.2)    │
                                                       │
Phase 6 (Pipeline) depends on 5.3, 4.2 ──────────────┤
  ├── 6.1 Scheduler                                    │
  └── 6.2 Full Pipeline Integration (depends on 6.1)   │
                                                       │
Phase 7 (Delivery) depends on 6.2 ────────────────────┤
  ├── 7.1 Telegram Client                              │
  ├── 7.2 Premium/Free Routing (depends on 7.1)        │
  └── 7.3 Alert Delivery (depends on 7.1, 4.2)         │
                                                       │
Phase 8 (Dashboards) depends on 7.2 ──────────────────┤
  ├── 8.1 Grafana Dashboards                           │
  └── 8.2 Grafana Alert Rules                          │
                                                       │
Phase 9 (Launch) depends on 8.2 ──────────────────────┘
```

Note: Phases 2 and 3 can be developed in parallel — they're independent until Phase 4 connects them. Phase 1.2 and 1.3 can also be parallel.

---

## What's Constant Across All Phases

- **Unit tests** written with every module, not after
- **CI green** before moving to the next sub-phase
- **Tracing spans** on every significant operation from the first module
- **Fixtures captured** from real API calls during manual verification, committed to repo for CI
- **DESIGN.md updated** when implementation reveals design adjustments
