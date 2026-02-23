# Kairex — Design Document

Working design document. Evolves through discussion.

---

## Product

AI-powered crypto market analysis delivered via Telegram. Monitors multiple assets across three timeframes (5m/1h/1d), synthesizes analysis through an LLM orchestrator, and delivers structured reports and real-time alerts to subscribers.

Core value: attention savings for active traders who can watch 2–3 assets themselves but not 9+ simultaneously.

---

## Channel Structure

### Free Public Channel

Top-of-funnel. Discoverable, shareable, builds public track record.

**Daily content:**
- Evening recap — references what the morning report flagged and how it played out. Genuinely useful on its own, not a crippled teaser. Demonstrates the accountability loop publicly.

**Periodic content:**
- Weekly scorecard — every call made, what hit, what missed, running accuracy. Transparency as trust signal.
- Occasional morning report — event-driven, not random. Sent when something genuinely notable is happening (big overnight move, regime shift, correlation breakdown). Roughly weekly frequency, scarce enough to not cannibalize paid tier.
- Contextual upsell nudges — attached to teasers that leave the reader wanting more. Links to paid channel invite.

### Paid Private Channel (Telegram Stars Subscription)

Full product. Gated via native Telegram Star subscription on private channel invite link.

**Daily — three scheduled reports:**
- **8:30 AM ET — Pre-Market** — overnight recap, what's setting up, what to watch for the day.
- **12:00 PM ET — Midday** — how the morning played out against the pre-market read, updated picture, anything new developing.
- **8:00 PM ET — Evening Recap** — day scorecard on how calls played out, overnight watch.

The three reports form a narrative thread: morning sets expectations, midday updates them, evening scores them.

**Real-time — 24/7 alerts:**
- Significant events pushed immediately, contextualized against the most recent report.
- Reduced noise overnight — only genuinely significant moves fire outside active hours.

**Weekly — Sunday afternoon:**
- One cohesive weekly briefing: week in review, prediction scorecard, asset-by-asset summary, week ahead.
- No daily reports on Sunday — the weekly briefing is the Sunday content. Monday pre-market picks up the thread.

**Content design for each report type is detailed in the Product Design section below.**

### Design Principles

- Free vs paid gap is **timing and depth**, not quality. Free tells you what happened. Paid tells you what's coming and alerts you when it arrives.
- The free channel is the marketing. Accurate evening recaps that reference morning calls build credibility. Conversion trigger is demonstrated value, not persuasion.
- Free channel doubles as a public archive — anyone evaluating subscription can scroll back and verify the track record.

---

## Subscription Model

Native Telegram Stars subscription on private channel invite link. Telegram handles payment, access gating, auto-renewal, and removal on lapse.

- Monthly billing (only option with Stars)
- ~35% revenue cut (Apple/Google store fees baked into Star pricing)
- Withdrawal via TON through Fragment, 21-day hold
- Price TBD — balancing user-facing cost (~$49 target) against net revenue after platform cuts

Accepted tradeoff: lower net revenue per subscriber in exchange for zero subscription infrastructure to build/maintain. Revisit self-managed payments (Stripe via Bot Payments API) only if/when scale justifies the engineering.

---

## Architecture

### Core Principle: Collection and Analysis are Separate Concerns

The system is built in layers that don't know about each other. The storage layer is the interface between them.

### Layer 1 — Collection (Continuous)

Collector modules run continuously, connecting to external data sources, normalizing data, and writing to storage. Each collector is independent — adding a new data source means building a new collector module. Nothing else changes.

**Data sources (initial):**
- Price candles (5m/1h/1d) via Binance Spot WebSocket + REST
- Funding rates (8h interval) via Binance Futures API
- Open interest (hourly) via Binance Futures API
- Fear & Greed Index (daily) via Alternative.me API
- BTC/ETH dominance + total market cap (daily) via CoinGecko API

**Tiered candle retention** (carried from trading brain architecture):
- 5m candles: 30 days → aggregated to 1h
- 1h candles: 1 year → aggregated to 1d
- 1d candles: 7 years

Collection also **emits events** as data arrives, feeding the live analysis layer.

### Layer 2 — Storage

The interface between collection and analysis. Both sides talk to an abstraction, not directly to a specific database. Collectors write, analyzers read. Storage handles retention, aggregation, and pruning.

Storage holds two categories of data:

**Market data** — raw data from collectors. Candles, funding rates, open interest, liquidation levels, sentiment indices. This is the numerical foundation analysis scripts compute against.

**System output data** — structured records of the system's own analytical outputs. Setups, predictions, key levels, alerts fired. This closes the feedback loop: the system can reference its own prior analysis, score its calls against outcomes, and maintain continuity across reports.

Both categories are queryable by analysis scripts in the same way. From the analysis layer's perspective, "what did we say this morning" is just another data query alongside "what did BTC do today."

### Layer 3 — Analysis (Two Modes)

**Periodic analysis** — scheduled scripts that run on intervals. Read from storage, compute across the full dataset, produce structured outputs (morning reports, evening recaps, weekly deep dives). Can be added, removed, rewritten, or rescheduled without touching collection.

**Live analysis** — scripts that evaluate incoming data in real-time via the event stream from the collection layer. Trigger alerts when conditions are met (volume spikes, RSI divergence resolution, key level breaks, funding rate extremes near liquidation clusters). Same extensibility — add new alert scripts without modifying anything else.

### Layer 4 — Delivery (Telegram)

Built on top of analysis outputs. Periodic analysis produces reports → formatted and posted to channels on schedule. Live analysis triggers alerts → formatted and pushed immediately. Delivery logic handles routing (free channel vs paid channel), formatting, and upsell placement. Decoupled from analysis — the analysis scripts produce structured data, the delivery layer decides how and where to present it.

### Extensibility

Each layer is independently extensible:
- New data source → new collector module, everything downstream can immediately query it
- New analysis → new periodic or live script, reads from existing storage
- New output format → new delivery template, consumes existing analysis outputs
- Nothing requires rewriting the full codebase — you build on top of what's running

### Design Concerns

**Schema evolution:** Structured output schemas will change as the product iterates. Old records in storage were written against old schemas. Requires either schema versioning or forward-compatible design from the start.

---

## LLM Integration

### Principle: Maximize Awareness, Minimize Direction

The LLM never does math. Analysis scripts compute all indicators and metrics from raw data — numbers only, no labels, no interpretation, no editorial thresholds. The LLM receives the complete numerical picture and does all reasoning itself. A funding rate of +0.067% is just a number — the LLM decides whether that's notable given the full context of price action, open interest, liquidation levels, and recent history.

The prompt shapes output format and system voice. It never directs analytical conclusions.

### Structured Output as Primary Interface

The LLM's primary output is entirely structured data. Every response follows a defined schema with typed fields. The editorial prose lives in narrative string fields within that structure. No parsing — the LLM produces the structure directly, which Anthropic's models do reliably.

**The structure serves both storage and delivery simultaneously.** Machine-readable fields (trigger levels, setup status, asset, direction) feed storage for setup tracking, accountability queries, and future context. Narrative fields (editorial text) feed delivery for Telegram formatting. Same output, two consumers.

Example — a morning report response:
```
regime_status: "range_bound"
regime_duration_days: 4
regime_narrative: "Range-bound consolidation. No change in 4 days..."
setups: [
  {
    asset: "ETH", direction: "short",
    trigger_condition: "price_below", trigger_level: 1880.0,
    target_level: 1820.0, invalidation_level: 1950.0,
    narrative: "ETH is the one to watch today..."
  }
]
market_narrative: "Rest of the market is unremarkable..."
significance: {
  magnitude: 0.4,       # how big in market terms — pure impact
  surprise: 0.7,        # how unexpected given the current read
  regime_relevance: 0.3  # does this potentially change the bigger picture
}
```

**Significance ratings** are analytical fields the LLM produces naturally as part of its assessment — not a routing decision. The LLM doesn't know these are used for free channel gating. It's just answering analytical questions it's already equipped to answer: how impactful is this, how surprising, how relevant to the regime. These fields appear on every report and alert output.

**Why this works:**
- **Reliability.** No prose parsing. The schema is the contract.
- **Forces rigor.** Fields like `invalidation_level: float` mean the LLM can't hand-wave. The structure is a forcing function for analytical precision.
- **Internal consistency.** Writing "trigger level $1,880" in prose while filling `trigger_level: 1880.0` in the schema forces the LLM to think precisely about what it's saying.
- **Clean downstream.** Storage ingests structured records directly. Delivery renders from narrative fields. Future analysis scripts query exact fields.

### Data Flow

```
Analysis scripts produce pre-computed numerical context
  → LLM receives context + output schema
  → LLM returns fully structured response
  → Structured response writes to storage (system output data)
  → Structured response feeds to delivery (formatted for Telegram)
```

One output, two consumers, no parsing.

### The Accountability Loop

Because the system's outputs are structured records in storage, recapping and scoring are just queries:
- Evening recap queries morning/midday setups + actual price action → LLM receives both, scores the day
- Weekly briefing queries all setups/predictions for the week + outcomes → LLM synthesizes performance review with historical context
- The free channel's credibility comes from this loop being mechanical, not editorial

### Setup Tracking — Live Alerting Without Per-Candle LLM Calls

The LLM is only called for the three daily reports, alert generation (when triggered), and the weekly briefing. It is never called on a per-candle basis.

**Setup creation:** When the LLM produces setups in structured output, each setup includes machine-readable trigger conditions (asset, condition type, level, volume requirements). These are precise enough for code to evaluate.

**Live evaluation:** The live analysis layer (pure code, no LLM) checks incoming data against active setup trigger conditions on every candle. When a condition is met, it fires — bundling the trigger event with context from storage (which setup, recent report data, current market state) and calling the LLM once to produce the alert message.

**Alert deduplication:** The live layer tracks fired alerts per setup with cooldown periods to prevent spam when price hovers around a trigger level.

### Weekly Context — No Nightly Consolidation Needed

The structured output store eliminates the need for a nightly consolidation cycle. By Sunday, the week's data is already structured records in storage: every setup with status and outcome, every prediction, every alert fired, every regime assessment. The weekly analysis script queries these directly and builds a compact, high-signal context for the LLM — no need to feed 21 raw report transcripts.

The weekly is a higher-token LLM call but fixed cost and only once per week. The agent constructs its own context by querying structured output history, including past weeks where relevant for trend and regime continuity.

### Scheduling & Delivery

Reports are generated with a time buffer before their scheduled delivery time. If generation completes early, the report is held and released on schedule. If generation runs past the target time, the report is sent immediately on completion. This keeps delivery times consistent without risking delays from LLM latency.

### Model Selection

Claude Opus for all report types and alerts. The call volume is sparse (3 daily + weekly + occasional alerts) and the product's value comes from synthesis quality and deep context utilization. No reason to compromise on model capability.

### Cost Model

All reports and alerts are broadcast — one LLM call serves all subscribers. Revenue scales linearly with subscribers. LLM costs are fixed per day (3 report calls + alerts + 1 weekly). Infrastructure costs do not grow with audience size.

### Prompt Architecture — Three Layers

Carried from the trading brain's proven design:

- **Layer 1 — Identity:** The analyst's character. Sharp, direct, honest, probabilistic. Consistent voice across all outputs.
- **Layer 2 — System Understanding:** What data is available, what the output schema means, what the product is.
- **Layer 3 — Per-Call Context:** Pre-computed market data, active setups, recent outputs from storage, and the specific output schema for this report type.

---

## Product Design

### Voice & Format Philosophy

The product is an **analytical editorial**, not a dashboard. The system has been watching 9+ assets across three timeframes continuously. Each output is the analyst's informed read — sharp, direct, opinionated, doesn't waste your time.

- **Numbers are citations, not content.** They appear inline as evidence supporting the analysis, not in tables for the reader to interpret.
- **Every asset gets covered** but proportionally — where something is developing, say more. Where nothing is happening, say that and move on. Group unremarkable assets naturally ("alts drifting in lockstep, nothing to differentiate").
- **Opinions held honestly.** When uncertain, say so. When wrong, own it. Probabilistic, not hedged.
- **Readable in 30 seconds.** Respects the reader's time and intelligence. These are active traders — they know what RSI means. They don't need explanations, they need the read.

This voice carries through every output: reports, alerts, weekly briefings. It reads like a person — a sharp analyst who works 24/7, covers everything, and tells you what matters.

### Pre-Market Report (8:30 AM ET)

Light analysis spread across the market. Not deep dives, but a quick informed read on everything. The analyst's morning briefing before the day unfolds.

**Opens with regime context** — not a one-word label but a living read. "Range-bound consolidation. No change in 4 days. BTC holding $66.3K–$69.4K on declining volume. Nothing suggesting a breakout is imminent — the market is waiting for a catalyst."

**Body is editorial, not data.** The analyst works through the market, dwelling on what's interesting, moving quickly past what isn't. Numbers woven in as evidence:

> ETH is the one to watch today. Funding has been creeping up (+0.067% last period) while price drifts toward $1,880 where there's a $22M long liquidation cluster. If that level breaks, forced selling accelerates the move. Not there yet but it's tightening.
>
> SOL continues to be relative strength — up 4.8% in 24h while everything else chops. Approaching $144 which has been resistance for a week. A clean break on volume would be notable.
>
> Rest of the market is unremarkable. ADA, DOGE, XRP, DOT, AVAX all drifting within yesterday's ranges on low volume. No setups, no signals.

**Ends with what to watch** — specific, actionable, concise:

> Watching two things today:
> — ETH below $1,880 on volume → likely accelerates to $1,820 (next cluster). Invalid above $1,950.
> — SOL above $144 hourly close with volume → targets $152. Invalid below $138.

The whole thing reads in 30 seconds. The model covered every asset, gave its read, flagged what matters, and told you what it's watching.

### Midday Report (12:00 PM ET)

The first check-in. The analyst has been watching all morning and is updating you against the pre-market read. This is a **continuation**, not a fresh assessment — it keeps the narrative thread alive.

**Shortest of the three reports.** When the morning's read is playing out as expected, it's brief — setups still in play, nothing new. When something's changed or broken, it earns more words and the tone shifts to match the urgency.

**Structure follows the morning naturally:**
- How the morning's calls are playing out (the bulk of the update)
- Anything new that's developed
- Updated levels if the picture has changed

**Quiet day example:**

> Morning's ETH call is in play. Price tested $1,890 twice in the past two hours on rising volume. Hasn't broken $1,880 yet but the bids are thinning. Funding ticked up again to +0.072%. Still watching.
>
> SOL faded. Rejected at $143.50 on weak volume — not the breakout we wanted. Backing off to $141. Setup still valid but momentum isn't there right now.
>
> BTC doing nothing. $67.8K, range intact. No new setups. Morning levels still the ones to watch.

**Active day example:**

> ETH broke $1,880 at 10:47 AM on 3.2x volume. The liquidation cascade I flagged this morning is playing out — $22M in longs got flushed. Down to $1,845 now.
>
> Everything else reacted. BTC slipped to $67.1K in sympathy but is holding. Alts down 1-2% across the board. Classic risk-off ripple from an ETH-led move.
>
> Updated watch: ETH $1,820 is the next level. If that holds, the flush is done and you're looking at a bounce setup. If it breaks, $1,760.

### Evening Recap (8:00 PM ET)

Closes the loop on the day. The analyst looks back at everything — morning calls, midday update, alerts that fired — and gives an honest debrief plus overnight context. This is where daily accountability lives most visibly.

**Leans slightly longer than midday** because it wraps a full day and sets up overnight. Still editorial, not data.

**Structure:**
- How the day's calls played out — honest, direct, owns misses
- Notable moves or developments through the day
- Overnight watch — what to monitor through Asia/Europe sessions, carries forward or updates active setups

**Mixed day example:**

> Two calls today, one worked. ETH broke $1,880 mid-morning exactly as described — the liquidation cascade played out to $1,838 before bouncing. If you caught the alert at 10:47, that was a clean $50 move in under an hour. Currently $1,862 in recovery.
>
> SOL didn't trigger. Rejected at $143.50 and drifted back to $140. Setup isn't invalidated — $138 was the line and we're above it — but momentum clearly isn't there. Carrying it forward with less conviction.
>
> Overnight: ETH is the story. Watch whether it holds $1,850 through Asia or rolls over for another leg. Funding hasn't reset yet (+0.058%) — still a risk for longs. BTC range continues, $66.3K–$69.4K.

**Quiet day example:**

> Quiet day. Neither setup triggered. ETH tested $1,890 twice but never committed — volume wasn't there. SOL drifted sideways around $141. BTC range continues.
>
> Day 5 of consolidation. Market is clearly waiting for something. Setups carrying forward unchanged. Same overnight levels — ETH $1,880 and SOL $144 remain the triggers.

### Real-Time Alerts (24/7)

The shoulder tap. Something meaningfully changed in the market picture and waiting for the next scheduled report means you missed it. Every alert passes the test: "would a serious trader want to be interrupted for this?"

**Trigger criteria — events, not readings:**
- Surprise selloff — something broke, price moving fast
- Major breakout after prolonged compression — a range just resolved
- Recovery after significant drop — the flush might be done
- Liquidation cascade in progress — forced selling accelerating a move
- Massive funding shift — derivatives picture just flipped
- Correlation breakdown — an asset suddenly diverging from everything else
- Morning setup triggering — the thing we said to watch is happening now

**Not alert-worthy:** an indicator crossing a threshold, a slow drift, anything that's "interesting but not urgent." Those belong in the next scheduled report.

**Format: three beats, a few sentences.** What happened, what it means in context of the current read, what to watch next.

> ETH just broke $1,880 on 2.8x volume. This is the liquidation cluster flagged in this morning's pre-market — forced selling is likely accelerating the move. Next support $1,820. Watch for a volume climax to signal the flush is done.

**Overnight noise reduction:** During active hours (roughly 8 AM–10 PM ET), alert on meaningful events. Overnight, the bar is higher — only genuinely significant moves (5%+ swings, liquidation cascades, picture-changing developments). Garden variety stuff waits for the morning report.

### Weekly Briefing (Sunday Evening)

The zoom-out. The analyst steps back from the daily and looks at the bigger picture. This is the one report that earns 2–3 minutes of reading. The editorial of the week.

**Value:** Perspective traders can't get from being in the weeds every day. The daily tells you ETH dropped Tuesday and bounced Thursday. The weekly tells you that was just a retest of three-week support, it held, and the structure is intact. The "so what does all of this actually mean."

**Flow:**

**Scorecard (the hook)** — lead with it. How did the system do this week? Concrete numbers: setups triggered, hit rate, misses. Then an honest take on what was wrong and why. This is the section that gets screenshotted and shared. It's also what keeps subscribers through a bad week — owning misses publicly builds trust through drawdowns.

**Week narrative** — not a day-by-day recap, they lived through that. What was the *story* of the week? Reframes a chaotic week into something coherent. "This was a week about ETH. Monday's funding creep set up Tuesday's liquidation cascade. The rest of the week was about whether the damage was done. It was — $1,820 held cleanly."

**Regime check** — has the macro picture changed? Not the one-liner from the daily header but a considered take on whether the trend structure is evolving. This is where shifts get flagged with weight.

**What would change my mind** — the system states its current thesis and explicitly says what would make it wrong. "I think we're still range-bound, but a daily close above $70K on rising OI would make me reconsider." Shows intellectual honesty and gives traders specific things to watch that challenge the prevailing read. Invites thinking, not just consumption.

**Week ahead** — the forward-looking payoff. After the recap and analysis, what does it all add up to? Setups, levels, scenarios for the coming week. Sends people into Monday with a framework.

---

## Free Channel Strategy

### Single Pipeline, Two Consumers

There is one content pipeline — the premium pipeline. Every report and alert dispatches as a premium event. The premium channel always consumes these events. The free channel sees the same event stream and selectively routes, condenses, or passes through.

The free channel never has its own content generation logic. It's always a transformation of premium content. This means:
- Free content automatically stays in sync with premium format changes
- New premium features are immediately visible to the free pipeline — just configure routing
- No separate analysis runs or duplicate LLM calls for free content

### Routing Configuration

Each premium event type has a configurable free channel behavior:

**Always route (condensed):**
- Evening recap → condensed version (produced in the same LLM call via `free_narrative` fields). Highlights, setup outcomes, enough to demonstrate accountability. Not enough to trade on.

**Always route (pass through):**
- Weekly scorecard section → pulled from Sunday editorial. Hit rate, honest accounting. The trust builder.

**Threshold-gated (configurable):**
- Morning reports and alerts carry significance ratings from the LLM's structured output: `magnitude` (0.0–1.0), `surprise` (0.0–1.0), `regime_relevance` (0.0–1.0). The LLM assigns these as part of its normal analytical assessment — it doesn't know they're used for routing.
- The Rust routing layer evaluates these ratings against configurable threshold rules. Example config:

```toml
[free_channel.morning_report]
# Route to free if any rule matches
rules = [
  { field = "magnitude", op = ">", value = 0.7 },
  { fields = ["surprise", "regime_relevance"], op = "all_above", value = 0.5 },
]

[free_channel.alerts]
rules = [
  { field = "magnitude", op = ">", value = 0.8 },
  { fields = ["magnitude", "surprise"], op = "all_above", value = 0.6 },
]
```

- We control the free channel character entirely through config. Tunable toward "only big moves" or "anything surprising" or "only regime shifts" — no prompt changes, no extra LLM calls.

**Upsell placement:** Contextual, attached to condensed content that references premium content the free user didn't see. "Full morning briefings and real-time alerts in the premium channel" with invite link.

### Extensibility

Adding a new premium feature = adding a new event type. The free pipeline sees it automatically. Configure routing when ready — no redesign of the free channel required.

---

## Data Sources & Processing

### Data Sources

| Source | Data | API | Auth | Update Frequency |
|--------|------|-----|------|-----------------|
| Binance Spot | Price candles (5m/1h/1d) | REST + WebSocket | None (public) | Real-time (WS), backfill (REST) |
| Binance Futures | Funding rates | REST `/fapi/v1/fundingRate` | None (public) | Every 8h (00:00/08:00/16:00 UTC) |
| Binance Futures | Open interest | REST `/fapi/v1/openInterest`, `/futures/data/openInterestHist` | None (public) | Hourly |
| Alternative.me | Fear & Greed Index | REST | None | Daily |
| CoinGecko | BTC/ETH dominance, total market cap | REST | Free API key (Demo tier) | Daily (cached every 10min) |

No trading relationship needed — all data is public market data for analysis only.

Liquidation data deferred. Can be added later via CoinGlass API if the product warrants it.

### Asset Universe

~9-10 liquid major crypto pairs against USDT on Binance. Focused, curated list — the analyst's value comes from covering a tight universe deeply, not scanning hundreds of tokens superficially. Roughly: BTC, ETH, SOL, XRP, DOGE, ADA, AVAX, LINK, DOT — finalized closer to launch based on what's actually being traded.

The asset list is a config-level concern. Adding or removing an asset doesn't require code changes — every collector, analysis script, and delivery template works off the configured symbol list.

### Gap Detection & Backfill

A property of the collection layer, not a special startup routine. Each collector:

1. On startup, checks the most recent timestamp in storage for its data type per asset
2. Detects gaps between last stored data and current time
3. Backfills from REST API with time range parameters to fill gaps
4. Switches to live streaming once caught up

Same mechanism handles all cases: cold start (backfill years of daily candles), restart after downtime (backfill hours), brief network interruption (backfill minutes), WebSocket disconnection (detect gap on reconnect, backfill automatically).

Binance REST API supports `startTime`/`endTime` parameters with pagination for candles, funding rates, and open interest — all backfillable.

### Analytical Cold Start

Option B — natural start. No silent running period. On day one, the system output store is empty (no prior setups, predictions, or reports). The LLM receives empty history fields and produces a fresh market read based purely on numerical data. The narrative thread builds naturally — by day three the system is self-referencing, by end of week one the weekly briefing has a full week of structured data to synthesize.

### Tiered Retention

| Timeframe | Resolution | Retention | Source |
|-----------|-----------|-----------|--------|
| 5m | 5-minute | 30 days | WebSocket + REST backfill |
| 1h | 1-hour | 1 year | Aggregated from 5m |
| 1d | 1-day | 7 years | Aggregated from 1h |

Funding rates, open interest, dominance, Fear & Greed retained at native resolution with reasonable pruning (TBD).

---

## Identity

The voice is Kairex but it never says the name. The name lives on the channel and the brand, not in the content. The analyst just speaks.

### Core Character

**Radical honesty.** Does not rationalize calls. When a setup missed, it says so plainly. Does not cherry-pick results or find patterns that aren't there. Acknowledges sample size limitations. A miss is a miss.

**Comfort with uncertainty.** Comfortable saying "I don't have a strong read here" or "this could go either way." Does not force conclusions from thin data. But does not use uncertainty as an excuse to avoid making calls — knows the difference between needing more data and avoiding commitment.

**Probabilistic thinking.** Thinks in likelihoods, not certainties. A setup that hits invalidation isn't a bad call — it's risk management working. What matters is whether the calls have edge over many weeks. Frames everything in probabilities, never in guarantees.

**Long-term orientation.** Thinks in compounding credibility. Individual calls are data points, not verdicts. The track record over months matters more than any single day. This is what makes the scorecard honest — bad weeks are openly acknowledged because the long-term record is what builds trust.

### Voice

Reads like a senior analyst at a trading desk sending you notes. Smart, direct, been watching the screens all night, tells you what matters in the time it takes to drink your coffee. Has opinions and owns them.

- **Sparse first-person.** "ETH is the one to watch" not "I think ETH is the one to watch." The analysis speaks for itself. First-person only when natural — "watching two things today," "this is what was flagged this morning."
- **No persona performance.** Doesn't perform confidence or humility. Just honest about what it sees.
- **Assumes competence.** Talks to traders who know what RSI means. Never pedagogical, never hedges with disclaimers. Sharp colleague, not a newsletter.
- **Not financial advice.** Frames as observations and setups, not recommendations. "This setup is developing" not "you should buy." The distinction is natural in the voice, not lawyerly.

---

## Tech Stack

### Language Split

**Rust** — the core system. Everything that runs 24/7 and needs to be bulletproof.

**Python** — the math engine. Computes indicators and builds LLM context. Called as subprocesses by the Rust core with JSON in/out.

The boundary is clean: Python does computation, Rust does everything else. Python never touches storage, APIs, scheduling, or delivery. It receives data, computes on it, returns numbers.

### Rust Core Responsibilities

- **Collection** — Binance WebSocket connections (real-time candles/ticks), REST polling (funding rates, OI, Fear & Greed, dominance), gap detection and backfill
- **Storage** — SQLite reads/writes, tiered retention, aggregation, pruning, system output store
- **Scheduling** — report timing with generation buffer, cron-style job management
- **Live evaluation** — checks computed indicator values against alert conditions and LLM setup triggers, alert deduplication with cooldowns
- **LLM calls** — Anthropic REST API directly (no official Rust SDK), structured output via serde, schema validation
- **Delivery** — Telegram Bot API for channel posting, free/premium routing, upsell placement
- **Observability** — tracing crate for structured hierarchical spans, OpenTelemetry integration, metrics

### Python Script Responsibilities

- **Indicator computation** — RSI, Bollinger bands/bandwidth, ADX, EMA ribbon, volume ratios, MACD, and any future indicators
- **LLM context building** — assembling the pre-computed numerical picture from raw data into structured context for each report type
- Called as subprocesses: Rust queries storage, serializes relevant data as JSON, pipes to Python, Python computes and returns JSON
- Each script declares its data requirements (which timeframes, how much history) so the Rust core passes only what's needed

### Python Dependencies

pandas, numpy, ta (technical analysis), scipy. Stable, mature libraries with slow release cycles. Installed in the Rust container image.

### Key Libraries / Crates

**Rust:**
- `tokio` — async runtime
- `reqwest` — HTTP client (Binance REST, Anthropic API, Telegram API, CoinGecko, Alternative.me)
- `tokio-tungstenite` — WebSocket client (Binance streams)
- `rusqlite` — SQLite (with bundled feature for self-contained builds)
- `serde` / `serde_json` — structured serialization, LLM output schemas
- `tracing` + `tracing-subscriber` — structured observability
- `opentelemetry` — distributed tracing export
- `chrono` — time handling, scheduling
- `tokio-cron-scheduler` or equivalent — scheduled job management

**Python:**
- `pandas` — DataFrame operations for candle data
- `numpy` — numerical computation
- `ta` — technical indicator library
- `scipy` — statistical functions

### Database

SQLite. Single-file, no external server, easy backup. Same reasoning as the trading brain — single-writer system with moderate data volumes.

JSON columns for structured LLM outputs to handle schema evolution gracefully. Full structured output stored as JSON, specific fields queryable via SQLite JSON path expressions.

### Deployment

Docker Compose on a US VPS. Application container (Rust + Python) alongside the full observability stack.

```yaml
services:
  kairex:
    build: .
    volumes:
      - ./scripts:/app/scripts:ro      # Python analysis scripts (hot-swappable)
      - ./config:/app/config:ro        # System config (asset list, schedules, thresholds)
      - ./prompts:/app/prompts:ro      # LLM prompts, identity, output schemas
      - ./data:/app/data               # SQLite database (persistent)
    env_file: .env                     # Secrets (deployed via Ansible Vault)

  # Observability stack
  grafana:       # Dashboards + alerting
  tempo:         # Trace storage (receives OTLP from app)
  prometheus:    # Metrics (scrapes app /metrics)
  loki:          # Log aggregation
  alloy:         # Log collector (ships container logs to Loki)
```

### VPS Management — Ansible

All VPS configuration is reproducible via Ansible playbooks run from the dev machine. The VPS is never manually configured.

**Deploy user model:**
- Dedicated `deploy` user with SSH key auth only
- Passwordless sudo for Docker commands only (not full root)
- Docker group membership
- Root SSH login disabled after deploy user is confirmed

**Secret management:** Ansible Vault. Secrets encrypted in the repo, decrypted at deploy time, written to `.env` on the VPS for Docker Compose. Safe to commit.

**Playbook structure:**

```
ansible/
  inventory/
    production.ini              # VPS host, deploy user, SSH key path
  playbooks/
    harden.yml                  # SSH hardening, firewall, fail2ban, deploy user
    wireguard.yml               # WireGuard server setup
    docker.yml                  # Docker + compose installation
    deploy.yml                  # Deploy/update the app + observability stack
    teardown-wireguard.yml      # Clean WireGuard removal (post-dev phase)
  roles/
    hardening/                  # SSH config, ufw, fail2ban, unattended-upgrades
    wireguard/                  # WireGuard server config
    docker/                     # Docker engine + compose plugin
    app/                        # Kairex compose stack + observability services
  vars/
    secrets.yml                 # Ansible Vault encrypted
  ansible.cfg                   # Local config
```

**Fresh VPS setup — run order:**

```bash
ansible-playbook playbooks/harden.yml       # 1. Secure the box
ansible-playbook playbooks/wireguard.yml    # 2. VPN for dev phase
ansible-playbook playbooks/docker.yml       # 3. Docker engine
ansible-playbook playbooks/deploy.yml       # 4. App + observability stack
```

After initial setup, only `deploy.yml` runs regularly — pull new images, restart services, verify health. All playbooks are idempotent.

### Hot-Swap vs Rebuild

| Change | Action | Downtime |
|--------|--------|----------|
| Python analysis script | Edit file on volume | Zero — next cycle picks it up |
| Config (assets, schedules, thresholds) | Edit file on volume | Zero — reload on next cycle or via signal |
| Prompts / schemas | Edit file on volume | Zero — next LLM call uses it |
| Rust core logic | Container rebuild + restart | Seconds |
| Python dependencies | Container rebuild + restart | Minutes (pip install) |
| Rust dependencies | Container rebuild | Longer (cargo build) |

Early phase: scripts iterate frequently, core rebuilds as features are developed. Steady state: scripts and dependencies stabilize, rebuilds become rare. Volume-mounted scripts provide hot-swap capability throughout.

### Observability

Enterprise-grade, designed in from the start — not bolted on. The full observability stack runs alongside the app on the same VPS in Docker Compose.

#### Infrastructure Stack

Five services alongside the main application:

- **Grafana** — dashboards, alerting rules, single pane of glass
- **Grafana Tempo** — trace storage backend, receives OpenTelemetry traces from the Rust app via OTLP gRPC
- **Prometheus** — scrapes metrics from the Rust app's `/metrics` endpoint
- **Grafana Loki** — log aggregation, receives structured JSON logs from containers
- **Grafana Alloy** — log collector, ships Docker container logs to Loki

#### What the Rust App Exposes

- **OpenTelemetry gRPC export** — `tracing` crate with `tracing-opentelemetry` layer exports spans to Tempo. Every operation is a span with structured fields.
- **Prometheus metrics endpoint** — `/metrics` on a lightweight HTTP server (axum or hyper). Prometheus scrapes on interval.
- **Structured JSON logs to stdout** — Docker captures, Alloy ships to Loki.

#### Span Hierarchy

Every operation is a structured span. Spans nest to form traces queryable in Tempo.

Report generation trace:
```
report_generation [report_type=morning, scheduled_for=1740000000]
  ├── market_data_query [symbols=10, staleness_check=true]
  ├── python_subprocess [script=build_context, timeout_ms=60000]
  │     ├── data_serialization [bytes=245000]
  │     └── process_execution [exit_code=0, duration_ms=1200]
  ├── llm_call [model=opus, schema=morning]
  │     ├── prompt_assembly [tokens_est=8500]
  │     ├── api_request [status=200, duration_ms=4200, input_tokens=8500, output_tokens=1200]
  │     └── output_validation [schema_version=v1, valid=true]
  ├── storage_write [output_id=142, setups_extracted=3]
  └── delivery
        ├── premium_channel [message_id=9928, chars=1840]
        └── free_channel_routing [magnitude=0.4, surprise=0.7, regime_relevance=0.3, routed=false]
```

Live evaluation cycle trace:
```
live_evaluation_cycle [cycle=28440, timestamp=1740000000]
  ├── python_subprocess [script=compute_indicators, duration_ms=800]
  ├── setup_evaluation [active_setups=5]
  │     ├── eval [asset=ETHUSDT, setup_id=42, triggered=false]
  │     └── eval [asset=SOLUSDT, setup_id=43, triggered=true, price=144.20]
  ├── system_rule_evaluation [rules=3]
  │     └── eval [rule=volume_spike, asset=BTCUSDT, fired=false]
  └── alert_generation [triggered=1]
        └── llm_call [model=opus, schema=alert, asset=SOLUSDT]
```

#### Prometheus Metrics

**Collection health:**
- `collection_last_update_timestamp{source, symbol}` — staleness detection
- `collection_gaps_detected_total` — gap detection counter
- `collection_backfill_duration_seconds` — backfill performance

**Live evaluation:**
- `live_cycle_duration_seconds` — 5m cycle performance
- `live_setups_active` — current active setup count
- `live_alerts_fired_total{alert_type, asset}` — alert fire rate

**LLM:**
- `llm_call_duration_seconds{report_type}` — latency distribution
- `llm_call_tokens_input{report_type}`, `llm_call_tokens_output{report_type}` — token usage
- `llm_call_errors_total{report_type, error_type}` — error rate by type

**Delivery:**
- `delivery_success_total{channel}`, `delivery_failure_total{channel}` — delivery reliability
- `delivery_latency_seconds{report_type}` — time from generation to delivery

**Scheduling:**
- `report_generation_duration_seconds{report_type}` — end-to-end pipeline duration
- `report_delivery_delay_seconds{report_type}` — actual delivery time minus scheduled time

**System:**
- `python_subprocess_duration_seconds{script}` — subprocess performance
- `python_subprocess_errors_total{script}` — subprocess reliability
- `sqlite_write_duration_seconds{table}` — storage performance

#### LLM Thought Storage

Separate from traces. Full structured input and output of every LLM call persisted to SQLite. Traces tell you *that* a call happened and how long it took. Thought storage tells you *what* was sent and *what* came back. Essential for debugging prompt issues, output quality, and schema evolution.

#### Operator Alerting

Grafana alerting rules evaluate Prometheus metrics and send notifications to a private operator Telegram channel. No custom alerting code in the Rust app — the app exposes metrics, Grafana decides when to alert. Examples: report delivery failed, collection lag exceeding 10 minutes, LLM call persistent errors, Python subprocess crashes.

Alerting logic lives in Grafana config, not application code. Tunable without rebuilds.

---

## Implementation Architecture

Mid-to-low level specification bridging design to code. Covers every boundary where data crosses between layers.

### SQLite Schema

**Market data tables:**

```sql
candles (
  symbol        TEXT NOT NULL,
  timeframe     TEXT NOT NULL,       -- '5m', '1h', '1d'
  open_time     INTEGER NOT NULL,    -- unix ms (Binance native)
  open          REAL NOT NULL,
  high          REAL NOT NULL,
  low           REAL NOT NULL,
  close         REAL NOT NULL,
  volume        REAL NOT NULL,
  source        TEXT NOT NULL,       -- 'ws', 'rest', 'aggregated'
  PRIMARY KEY (symbol, timeframe, open_time)
)

funding_rates (
  symbol        TEXT NOT NULL,
  timestamp     INTEGER NOT NULL,    -- unix ms
  rate          REAL NOT NULL,
  PRIMARY KEY (symbol, timestamp)
)

open_interest (
  symbol        TEXT NOT NULL,
  timestamp     INTEGER NOT NULL,
  value         REAL NOT NULL,       -- notional USD
  PRIMARY KEY (symbol, timestamp)
)

indices (
  index_type    TEXT NOT NULL,       -- 'fear_greed', 'btc_dominance', 'eth_dominance', 'total_market_cap'
  timestamp     INTEGER NOT NULL,
  value         REAL NOT NULL,
  PRIMARY KEY (index_type, timestamp)
)
```

**System output tables:**

```sql
system_outputs (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  report_type     TEXT NOT NULL,        -- 'morning', 'midday', 'evening', 'alert', 'weekly'
  generated_at    INTEGER NOT NULL,
  schema_version  TEXT NOT NULL,        -- 'v1', 'v2', etc.
  output          TEXT NOT NULL,        -- full structured JSON blob
  delivered_at    INTEGER,
  delivery_status TEXT DEFAULT 'pending' -- 'pending', 'delivered', 'failed'
)

active_setups (
  id                 INTEGER PRIMARY KEY AUTOINCREMENT,
  source_output_id   INTEGER NOT NULL REFERENCES system_outputs(id),
  asset              TEXT NOT NULL,
  direction          TEXT NOT NULL,     -- 'long', 'short', 'neutral'
  trigger_condition  TEXT NOT NULL,
  trigger_level      REAL NOT NULL,
  target_level       REAL,
  invalidation_level REAL,
  status             TEXT NOT NULL DEFAULT 'active',  -- 'active', 'triggered', 'invalidated', 'expired', 'superseded'
  created_at         INTEGER NOT NULL,
  resolved_at        INTEGER,
  resolved_price     REAL
)

fired_alerts (
  id             INTEGER PRIMARY KEY AUTOINCREMENT,
  setup_id       INTEGER REFERENCES active_setups(id),
  alert_type     TEXT NOT NULL,        -- 'setup_trigger', 'system_rule'
  fired_at       INTEGER NOT NULL,
  cooldown_until INTEGER NOT NULL,
  output_id      INTEGER REFERENCES system_outputs(id)
)
```

**Design choices:**

- **`active_setups` is a denormalized extraction** from `system_outputs`. When a new report arrives, Rust extracts its setups into this table and marks the previous report's setups for the same assets as `superseded`. Gives the live evaluation loop a fast, flat table to scan without JSON parsing on every 5-minute cycle.
- **`indices` is a single table** rather than one-per-type. Same shape, discriminated by `index_type`. Grows naturally as new data sources are added.
- **All timestamps are unix milliseconds** matching Binance native format. No timezone conversion at the storage layer.
- **Schema evolution** handled via `schema_version` on `system_outputs` + `#[serde(default)]` on new fields in Rust structs. Old records queryable via JSON path, new fields gracefully absent.

**Concurrency:** SQLite in WAL mode. Single write connection wrapped in `Arc<Mutex<Connection>>`, separate read connections for the analysis layer. Modest data volumes — not a bottleneck.

**Aggregation and pruning** runs as a background job on a daily schedule. Sequence: aggregate 5m → 1h, aggregate 1h → 1d, then prune expired candles. Never prune before aggregation.

### Python Subprocess Contract

**Two entry points, single batch call per invocation:**

```
scripts/
  compute_indicators.py    # called every 5m by live evaluation loop
  build_context.py         # called before each LLM report generation
  manifest.toml            # declares data requirements per entry point
  lib/
    indicators.py          # RSI, Bollinger, MACD, etc.
    context.py             # context assembly helpers
```

**Manifest declares data requirements** — Rust reads on startup (and on SIGHUP for reload), queries storage per declared requirements, serializes as JSON to stdin:

```toml
[compute_indicators]
description = "Compute technical indicators for live evaluation"
[compute_indicators.data]
candles_5m = { periods = 100 }
candles_1h = { periods = 50 }
candles_1d = { periods = 30 }

[build_context]
description = "Build pre-computed numerical context for LLM reports"
[build_context.data]
candles_5m = { periods = 288 }     # 24h
candles_1h = { periods = 168 }     # 7 days
candles_1d = { periods = 90 }      # 90 days
funding_rates = { periods = 90 }
open_interest = { periods = 720 }  # 30 days hourly
indices = { periods = 90 }
```

**Input shape** (JSON piped to stdin):

```json
{
  "assets": ["BTCUSDT", "ETHUSDT", "..."],
  "candles_5m": { "BTCUSDT": [{ "open_time": ..., "o": ..., "h": ..., "l": ..., "c": ..., "v": ... }] },
  "candles_1h": { "BTCUSDT": [...] },
  "funding_rates": { "BTCUSDT": [...] },
  "..."
}
```

**Output from `compute_indicators.py`** (JSON to stdout) — flat key-value per asset:

```json
{
  "BTCUSDT": {
    "rsi_14_5m": 42.3,
    "rsi_14_1h": 55.1,
    "bollinger_upper_1h": 68500.0,
    "bollinger_lower_1h": 66200.0,
    "bollinger_bandwidth_1h": 0.034,
    "macd_histogram_1h": -120.5,
    "adx_14_1d": 18.7,
    "volume_ratio_5m": 1.4
  },
  "ETHUSDT": { "..." }
}
```

Rust doesn't need to understand what these indicators mean — it evaluates them against trigger conditions. The indicator names are the contract between Python output and alert condition definitions.

**Output from `build_context.py`** — richer per-asset blocks with narrative-ready numerical context plus cross-market data. Feeds directly into the LLM prompt as Layer 3 per-call context.

**Invocation:** Rust spawns `python scripts/compute_indicators.py`, pipes JSON to stdin, reads JSON from stdout. Timeout enforced via tokio (30s for indicators, 60s for context building).

**Error handling:** Non-zero exit code or malformed JSON → log error with stderr content. For indicator computation: skip cycle, use last known values. For context building: retry once, then delay report generation with operator alert. Never silently proceed with bad data.

### Setup Lifecycle

Each report produces the **complete current setup list**. The LLM manages lifecycle through its structured output — it carries forward, modifies, or drops setups as its analysis evolves.

**When a new report is stored:**

1. Extract setups from the structured output
2. For each asset in the new setup list: mark any previous `active` setups for that asset as `superseded`
3. Insert new setups as `active`
4. For assets not in the new setup list: mark previous `active` setups as `expired` (the LLM chose not to carry them forward)

Steps 1–4 execute in a single SQLite transaction alongside the system output insert.

**Trigger resolution** — when the live layer detects a condition is met:

1. Mark setup `triggered`, record `resolved_at` and `resolved_price`
2. Insert into `fired_alerts` with cooldown period
3. Bundle context: setup details + current market state + recent report data
4. Call LLM to produce alert message
5. Store alert as system output, deliver to Telegram

**Invalidation** — when price crosses `invalidation_level`:

1. Mark setup `invalidated`, record `resolved_at` and `resolved_price`
2. Depending on config: fire an invalidation alert ("ETH short is invalidated, price broke $1,950") or note silently in the next report

**The live evaluation loop only queries `status = 'active'`.** Clean, fast, no state machine complexity in the hot path.

### Alert Condition Types

Two categories, both evaluated by Rust every 5 minutes against Python's computed indicator output:

**LLM setup triggers** — extracted from structured output, stored in `active_setups`. Condition types:

- `price_above`, `price_below` — price level checks
- `price_percent_move` — percentage move from setup creation price within a time window
- `indicator_above`, `indicator_below` — e.g., `trigger_condition: "indicator_below", trigger_field: "rsi_14_1h", trigger_level: 30.0`

Starting with price-level triggers. Indicator-based triggers added when the schema supports it and the LLM starts producing them naturally.

**System-level rules** — hardcoded Rust functions, configured via TOML:

```toml
[system_rules.volume_spike]
condition = "volume_ratio_5m > 3.0"
description = "Volume spike detected"

[system_rules.rsi_extreme]
condition = "rsi_14_1h < 20 OR rsi_14_1h > 80"
description = "RSI at extreme levels"

[system_rules.funding_extreme]
condition = "abs(funding_rate) > 0.1"
description = "Funding rate at extreme levels"
```

When a system rule fires: same flow as setup trigger — bundle context, call LLM for the alert message, store and deliver. The LLM contextualizes the event against everything else it knows.

Both categories feed into the same deduplication system. Cooldown periods per asset per alert type prevent spam when values hover near thresholds.

### Report Generation Pipeline

The full pipeline per scheduled report, with error handling at each step:

```
 1. Scheduler fires (buffer time before delivery)
    → Missed trigger: detect on next tick, generate immediately.

 2. Query storage for market data
    → SQLite error: retry once. Still failing → abort, system unhealthy.

 3. Detect data staleness (any collector's latest data older than expected)
    → Not a failure. Add staleness flags to LLM context so it can acknowledge gaps.

 4. Call Python subprocess (compute_indicators or build_context)
    → Timeout/bad output: retry once. Still failing → generate with raw data only, log prominently.

 5. Assemble LLM prompt (identity + system understanding + context + schema)
    → Missing prompt/schema files: abort, alert operator. Config error.

 6. Call Anthropic API
    → Rate limit: retry with backoff. API error: retry up to 3x.
      Persistent failure: abort, alert operator, deliver nothing.

 7. Validate structured output (serde deserialization + field validation)
    → Schema mismatch: log full response. Retry LLM call once. Still bad → abort.

 8. Store system output in database (single transaction with step 9)
    → SQLite error: retry. System unhealthy if persistent.

 9. Extract and update active setups (same transaction as step 8)
    → Supersede previous setups, insert new ones.

10. Hold until delivery time (if generated early)
    → If past delivery time: deliver immediately.

11. Post to premium Telegram channel
    → Retry with backoff. Track delivery_status. Persistent failure → alert operator.
      Report is stored — can be manually posted.

12. Evaluate free channel routing (significance thresholds from config)
    → If routed: format condensed version, post to free channel. Same retry logic.
```

Steps 8–9 are a single SQLite transaction. Steps 11–12 are independent but both depend on 8.

The entire pipeline runs as a structured async task with tracing spans at each step — every step observable in Grafana.

**Cross-report dependencies:** Midday and evening reference earlier reports. Context building queries `system_outputs` for today's prior reports. If the morning report failed, the query returns nothing — the LLM receives empty prior context and produces a standalone report. Degrades gracefully without special handling.

### Config & Prompt File Structure

```
config/
  assets.toml              # symbol list, per-asset display names
  schedules.toml           # report times, generation buffers, overnight hours
  thresholds.toml          # alert cooldowns, overnight multiplier, system rule definitions
  collection.toml          # polling intervals, WebSocket config, retry policies
  free_channel.toml        # routing rules, significance thresholds

prompts/
  identity.md              # Layer 1 — analyst character
  system.md                # Layer 2 — system understanding, data semantics
  schemas/
    morning.json           # output schema for morning report
    midday.json            # output schema for midday
    evening.json           # output schema for evening recap
    alert.json             # output schema for real-time alerts
    weekly.json            # output schema for weekly briefing

scripts/
  compute_indicators.py    # 5m indicator computation
  build_context.py         # pre-report LLM context building
  manifest.toml            # data requirements per script
  lib/
    indicators.py          # indicator computation modules
    context.py             # context assembly helpers
```

**Config reloading:** Rust reads config on startup and watches for changes via `notify` crate (or SIGHUP). Config loaded into `Arc<RwLock<Config>>` — readers clone current config at the start of each cycle, preventing mid-cycle inconsistency. Reload happens between cycles.

**Prompts and schemas:** Read fresh from the volume mount on each LLM call. No caching — file reads are cheap and this ensures hot-swap works immediately.

### Collection Layer Details

**WebSocket connection management:** ~10 assets × 3 timeframes = ~30 streams. Combined into a single Binance combined stream connection (`/stream?streams=btcusdt@kline_5m/ethusdt@kline_5m/...`). Well within Binance's 200-stream limit.

- Binance drops connections every 24h — automatic reconnection with exponential backoff
- Reconnection triggers gap detection to catch anything missed during the reconnect window
- Only final candles stored (Binance kline events include `is_final` flag). Intermediate candle data available to the live layer for evaluation but not persisted.

**REST polling:** Independent polling loops per data type with configured intervals. Funding rates every 8h, OI hourly, Fear & Greed daily, dominance daily. Consistent retry policy: retry with backoff on failure, skip to next interval after 3 failures, log prominently.

**Event emission:** In-process `tokio::broadcast` channel. Events carry the data payload (not just a notification) so the live layer doesn't need to re-query storage on every candle. Event shape: `CollectionEvent { source, symbol, data_type, payload, timestamp }`.

### Failure Visibility

**Primary:** Grafana dashboards and alerting via the observability stack. Logs, traces, and metrics for every system component. Operator monitors system health through dashboards.

**Secondary:** Private Telegram channel for operator alerts — system health notifications, delivery failures, persistent errors. Low-volume, high-signal. Fires only when something needs human attention.

### Testing & CI

Testing infrastructure goes in with the first line of code. CI runs on every push from the first buildable commit.

#### Unit Tests

Written alongside every module using Rust's built-in `#[test]` and `pytest` for Python.

**Storage layer** — highest value tests. Insert candles and query back, insert system output and extract setups, verify superseding logic, verify tiered retention aggregation (12 5m candles → one 1h candle). Run against in-memory SQLite — fast, no disk IO.

**Config parsing** — deserialize each TOML config into its Rust struct. Verify defaults, verify validation catches bad values.

**JSON serialization boundary** — the Rust/Python contract. Serialize sample data the way Rust pipes to Python, verify shape. Deserialize sample Python output into Rust structs. Uses fixture files representing the contract.

**Alert condition evaluation** — pure logic, no IO. Given indicator values and trigger conditions, does this setup fire? Does this system rule fire? Does deduplication work?

**LLM structured output parsing** — deserialize saved LLM responses (fixtures) into Rust structs. Verify all fields parse. Verify schema version handling. Verify responses missing new optional fields still parse (forward compatibility via `#[serde(default)]`).

**Free channel routing** — given significance scores and routing config, does the report get routed? Pure logic, exhaustively testable.

**Python indicator tests** — `pytest` for computation scripts. Given sample candle data, do RSI/Bollinger/MACD compute correctly? Verify against known values.

#### Integration Test Fixtures

```
tests/fixtures/
  binance/
    kline_ws_message.json
    kline_rest_response.json
    funding_rate_response.json
    open_interest_response.json
  python/
    compute_indicators_input.json
    compute_indicators_output.json
    build_context_input.json
    build_context_output.json
  llm/
    morning_report_response.json
    midday_report_response.json
    evening_report_response.json
    alert_response.json
    weekly_report_response.json
  config/
    assets.toml
    schedules.toml
    thresholds.toml
```

Fixtures are the contract. If any layer changes its output shape, the downstream fixture test breaks.

#### Integration Tests

Heavier tests exercising multiple layers with mocked external services. Separate target — `cargo test --test integration`.

- **Collection → Storage:** Feed mock WebSocket messages through the collection layer, verify they land in SQLite correctly.
- **Storage → Python → Evaluation:** Load fixture data into SQLite, run the real Python subprocess, feed results into evaluation logic, verify alert decisions.
- **Full report pipeline (mocked LLM):** Run the entire 12-step pipeline with a mock Anthropic API returning fixture responses. Verify report stored, setups extracted, delivery formatting correct.

#### CI — GitHub Actions

On every push and PR:

```
1. cargo fmt --check            # formatting
2. cargo clippy -- -D warnings  # lints
3. cargo test                   # unit tests
4. cargo test --test integration # integration tests
5. cd scripts && pytest         # Python tests
```

Full suite targets under 2 minutes. CI caches `target/` and Python venv between runs.

#### Dry-Run Mode

Config-level toggle. Runs the full pipeline but posts to a test Telegram channel (or logs the formatted message) instead of real channels. Enables end-to-end verification without affecting subscribers.

#### What's NOT in CI

Live API verification — real Binance WebSocket, real Anthropic calls, real Telegram delivery. These are manual checkpoints during development, documented in the roadmap as verification steps at specific milestones.
