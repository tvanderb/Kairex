"""Context assembly for LLM report generation.

Computes pre-processed numerical summaries from raw market data:
  - Per-asset: price changes, volume trends, funding rate stats, OI trends, key levels
  - Cross-market: fear & greed, dominance, total market cap

All values are ready-to-use numbers — the LLM focuses on interpretation, not arithmetic.
"""

import math

import numpy as np
import pandas as pd


def build_context(data: dict) -> dict:
    """Build LLM context from market data.

    Args:
        data: Input from Rust caller with full market data.

    Returns:
        {"assets": {per-asset context}, "market": {cross-market context}}
    """
    assets = data.get("assets", [])
    candles = data.get("candles", {})
    funding_rates = data.get("funding_rates", {})
    open_interest = data.get("open_interest", {})
    indices = data.get("indices", {})

    result_assets = {}
    for asset in assets:
        asset_candles = candles.get(asset, {})
        asset_funding = funding_rates.get(asset, [])
        asset_oi = open_interest.get(asset, [])
        result_assets[asset] = _build_asset_context(asset_candles, asset_funding, asset_oi)

    market = _build_market_context(indices)

    return {"assets": result_assets, "market": market}


def _build_asset_context(
    candles: dict[str, list],
    funding: list[dict],
    oi: list[dict],
) -> dict:
    """Build context block for a single asset."""
    ctx = {}

    # Price context from daily candles (primary) with 5m/1h for intraday
    ctx["price"] = _price_context(candles)
    ctx["volume"] = _volume_context(candles)
    ctx["levels"] = _level_context(candles)
    ctx["funding"] = _funding_context(funding)
    ctx["open_interest"] = _oi_context(oi)

    return ctx


def _price_context(candles: dict[str, list]) -> dict:
    """Compute price summary: current price and change percentages."""
    result = {}

    # Current price from most recent 5m candle (most up-to-date)
    for tf in ("5m", "1h", "1d"):
        tf_candles = candles.get(tf, [])
        if tf_candles:
            result["current"] = tf_candles[-1]["c"]
            break

    if "current" not in result:
        return result

    current = result["current"]

    # 24h change from 5m candles (288 × 5m = 24h)
    candles_5m = candles.get("5m", [])
    if len(candles_5m) >= 288:
        price_24h_ago = candles_5m[-288]["c"]
        result["change_24h_pct"] = _pct_change(price_24h_ago, current)
    elif len(candles_5m) >= 2:
        price_start = candles_5m[0]["c"]
        result["change_24h_pct"] = _pct_change(price_start, current)

    # 7d change from 1h candles (168 × 1h = 7d)
    candles_1h = candles.get("1h", [])
    if len(candles_1h) >= 168:
        price_7d_ago = candles_1h[-168]["c"]
        result["change_7d_pct"] = _pct_change(price_7d_ago, current)
    elif len(candles_1h) >= 2:
        price_start = candles_1h[0]["c"]
        result["change_7d_pct"] = _pct_change(price_start, current)

    # 30d change from 1d candles
    candles_1d = candles.get("1d", [])
    if len(candles_1d) >= 30:
        price_30d_ago = candles_1d[-30]["c"]
        result["change_30d_pct"] = _pct_change(price_30d_ago, current)
    elif len(candles_1d) >= 2:
        price_start = candles_1d[0]["c"]
        result["change_30d_pct"] = _pct_change(price_start, current)

    return result


def _volume_context(candles: dict[str, list]) -> dict:
    """Compute volume summary: 24h total and ratio vs 7d average."""
    result = {}

    # 24h volume from 5m candles
    candles_5m = candles.get("5m", [])
    if len(candles_5m) >= 288:
        last_24h = candles_5m[-288:]
        result["total_24h"] = _clean(sum(c["v"] for c in last_24h))
    elif candles_5m:
        result["total_24h"] = _clean(sum(c["v"] for c in candles_5m))

    # 7d average daily volume from 1h candles
    candles_1h = candles.get("1h", [])
    if len(candles_1h) >= 168:
        last_7d = candles_1h[-168:]
        total_7d = sum(c["v"] for c in last_7d)
        avg_daily_7d = total_7d / 7.0

        # Current day volume (last 24 1h candles)
        if len(candles_1h) >= 24:
            current_day = sum(c["v"] for c in candles_1h[-24:])
            result["vs_7d_avg_ratio"] = _clean(current_day / avg_daily_7d) if avg_daily_7d > 0 else None

    return result


def _level_context(candles: dict[str, list]) -> dict:
    """Compute key price levels: 24h/7d/30d high and low."""
    result = {}

    # 24h levels from 5m candles
    candles_5m = candles.get("5m", [])
    if len(candles_5m) >= 288:
        last_24h = candles_5m[-288:]
    elif candles_5m:
        last_24h = candles_5m
    else:
        last_24h = []

    if last_24h:
        result["high_24h"] = _clean(max(c["h"] for c in last_24h))
        result["low_24h"] = _clean(min(c["l"] for c in last_24h))

    # 7d levels from 1h candles
    candles_1h = candles.get("1h", [])
    if len(candles_1h) >= 168:
        last_7d = candles_1h[-168:]
    elif candles_1h:
        last_7d = candles_1h
    else:
        last_7d = []

    if last_7d:
        result["high_7d"] = _clean(max(c["h"] for c in last_7d))
        result["low_7d"] = _clean(min(c["l"] for c in last_7d))

    # 30d levels from 1d candles
    candles_1d = candles.get("1d", [])
    if len(candles_1d) >= 30:
        last_30d = candles_1d[-30:]
    elif candles_1d:
        last_30d = candles_1d
    else:
        last_30d = []

    if last_30d:
        result["high_30d"] = _clean(max(c["h"] for c in last_30d))
        result["low_30d"] = _clean(min(c["l"] for c in last_30d))

    return result


def _funding_context(funding: list[dict]) -> dict:
    """Compute funding rate summary: current, averages, trend, percentile."""
    if not funding:
        return {}

    result = {}
    rates = [f["rate"] for f in funding]
    timestamps = [f["ts"] for f in funding]

    result["current"] = _clean(rates[-1])

    # 7d average (funding every 8h = 21 entries per week)
    if len(rates) >= 21:
        result["avg_7d"] = _clean(np.mean(rates[-21:]))
    elif len(rates) >= 2:
        result["avg_7d"] = _clean(np.mean(rates))

    # 30d average (90 entries per 30 days)
    if len(rates) >= 90:
        result["avg_30d"] = _clean(np.mean(rates[-90:]))

    # Full-period average
    result["avg_all"] = _clean(np.mean(rates))

    # Trend: compare last 7d average to previous 7d average
    if len(rates) >= 42:
        recent = np.mean(rates[-21:])
        prior = np.mean(rates[-42:-21])
        if prior != 0:
            result["trend_pct"] = _clean((recent - prior) / abs(prior) * 100)

    # Percentile within all available data
    current = rates[-1]
    below = sum(1 for r in rates if r < current)
    result["percentile_90d"] = _clean(below / len(rates) * 100)

    # Count of consecutive same-sign funding periods
    sign = 1 if rates[-1] >= 0 else -1
    streak = 0
    for r in reversed(rates):
        if (r >= 0 and sign >= 0) or (r < 0 and sign < 0):
            streak += 1
        else:
            break
    result["same_sign_streak"] = streak

    return result


def _oi_context(oi: list[dict]) -> dict:
    """Compute open interest summary: current value and changes."""
    if not oi:
        return {}

    result = {}
    values = [o["value"] for o in oi]

    result["current"] = _clean(values[-1])

    # 24h change (24 hourly entries)
    if len(values) >= 24:
        result["change_24h_pct"] = _clean(_pct_change(values[-24], values[-1]))

    # 7d change (168 hourly entries)
    if len(values) >= 168:
        result["change_7d_pct"] = _clean(_pct_change(values[-168], values[-1]))

    # 30d change (720 hourly entries)
    if len(values) >= 720:
        result["change_30d_pct"] = _clean(_pct_change(values[-720], values[-1]))

    # 7d high/low
    if len(values) >= 168:
        last_7d = values[-168:]
        result["high_7d"] = _clean(max(last_7d))
        result["low_7d"] = _clean(min(last_7d))

    return result


def _build_market_context(indices: dict[str, list]) -> dict:
    """Build cross-market context from index data."""
    result = {}

    for index_type in ("fear_greed", "btc_dominance", "eth_dominance", "total_market_cap"):
        entries = indices.get(index_type, [])
        if not entries:
            continue
        result[index_type] = _index_context(entries)

    return result


def _index_context(entries: list[dict]) -> dict:
    """Compute summary for an index series: current, averages, changes."""
    if not entries:
        return {}

    values = [e["value"] for e in entries]
    result = {}

    result["current"] = _clean(values[-1])

    # 7d average
    if len(values) >= 7:
        result["avg_7d"] = _clean(np.mean(values[-7:]))
    elif len(values) >= 2:
        result["avg_7d"] = _clean(np.mean(values))

    # 30d average
    if len(values) >= 30:
        result["avg_30d"] = _clean(np.mean(values[-30:]))

    # 7d change
    if len(values) >= 7:
        result["change_7d"] = _clean(values[-1] - values[-7])

    # 30d change
    if len(values) >= 30:
        result["change_30d"] = _clean(values[-1] - values[-30])

    # Trend direction: compare recent 7d avg to prior 7d avg
    if len(values) >= 14:
        recent = np.mean(values[-7:])
        prior = np.mean(values[-14:-7])
        result["trend"] = "rising" if recent > prior else "falling" if recent < prior else "flat"

    return result


def _pct_change(old: float, new: float) -> float | None:
    """Compute percentage change, returning None if old is zero."""
    if old == 0:
        return None
    return (new - old) / abs(old) * 100


def _clean(value) -> float | None:
    """Convert a value to a clean float, returning None for NaN/inf."""
    if value is None:
        return None
    if isinstance(value, str):
        return value  # For trend direction strings
    try:
        f = float(value)
        if math.isnan(f) or math.isinf(f):
            return None
        return round(f, 8)
    except (TypeError, ValueError):
        return None
