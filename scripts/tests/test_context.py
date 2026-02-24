"""Tests for LLM context assembly correctness."""

import math
import sys

import numpy as np
import pytest

sys.path.insert(0, str(__import__("pathlib").Path(__file__).resolve().parent.parent))

from lib.context import (
    _funding_context,
    _index_context,
    _level_context,
    _oi_context,
    _pct_change,
    _price_context,
    _volume_context,
    build_context,
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def make_candles_5m(n=288, base=65000.0):
    """Generate 5m candles (288 = 24h)."""
    candles = []
    for i in range(n):
        c = base + i * 10.0
        candles.append({
            "ts": 1700000000000 + i * 300000,
            "o": c - 5, "h": c + 50, "l": c - 50, "c": c, "v": 100.0 + i,
        })
    return candles


def make_candles_1h(n=168, base=65000.0):
    """Generate 1h candles (168 = 7d)."""
    candles = []
    for i in range(n):
        c = base + i * 100.0
        candles.append({
            "ts": 1700000000000 + i * 3600000,
            "o": c - 10, "h": c + 200, "l": c - 200, "c": c, "v": 500.0 + i * 5,
        })
    return candles


def make_candles_1d(n=90, base=65000.0):
    """Generate 1d candles (90 = 3 months)."""
    candles = []
    for i in range(n):
        c = base + i * 500.0
        candles.append({
            "ts": 1700000000000 + i * 86400000,
            "o": c - 100, "h": c + 1000, "l": c - 1000, "c": c, "v": 10000.0 + i * 100,
        })
    return candles


def make_funding(n=90, base_rate=0.0001):
    """Generate funding rate entries (every 8h)."""
    return [
        {"ts": 1700000000000 + i * 28800000, "rate": base_rate + i * 0.00001}
        for i in range(n)
    ]


def make_oi(n=720, base_value=5_000_000_000.0):
    """Generate open interest entries (hourly)."""
    return [
        {"ts": 1700000000000 + i * 3600000, "value": base_value + i * 1_000_000.0}
        for i in range(n)
    ]


def make_index(n=90, base_value=50.0):
    """Generate index entries (daily)."""
    return [
        {"ts": 1700000000000 + i * 86400000, "value": base_value + i * 0.5}
        for i in range(n)
    ]


# ---------------------------------------------------------------------------
# Price context
# ---------------------------------------------------------------------------

class TestPriceContext:
    def test_current_price_from_5m(self):
        candles = {"5m": make_candles_5m(288)}
        ctx = _price_context(candles)
        # Last candle close: 65000 + 287 * 10 = 67870
        assert ctx["current"] == 67870.0

    def test_24h_change(self):
        candles_5m = make_candles_5m(288)
        candles = {"5m": candles_5m}
        ctx = _price_context(candles)
        # First candle close: 65000, last: 67870
        expected = _pct_change(65000.0, 67870.0)
        assert abs(ctx["change_24h_pct"] - expected) < 0.01

    def test_7d_change(self):
        candles = {"5m": make_candles_5m(288), "1h": make_candles_1h(168)}
        ctx = _price_context(candles)
        # 1h: first close 65000, last 65000 + 167 * 100 = 81700
        expected = _pct_change(65000.0, 67870.0)  # current from 5m
        assert "change_7d_pct" in ctx

    def test_30d_change(self):
        candles = {"5m": make_candles_5m(288), "1d": make_candles_1d(90)}
        ctx = _price_context(candles)
        assert "change_30d_pct" in ctx

    def test_empty_candles(self):
        ctx = _price_context({})
        assert ctx == {}

    def test_fallback_to_1h_for_current(self):
        candles = {"1h": make_candles_1h(10)}
        ctx = _price_context(candles)
        assert "current" in ctx


# ---------------------------------------------------------------------------
# Volume context
# ---------------------------------------------------------------------------

class TestVolumeContext:
    def test_24h_volume(self):
        candles = {"5m": make_candles_5m(288)}
        ctx = _volume_context(candles)
        # Sum of volumes: 100 + 101 + ... + 387 = 288 * (100 + 387) / 2 = 70056
        expected = sum(100.0 + i for i in range(288))
        assert abs(ctx["total_24h"] - expected) < 0.01

    def test_vs_7d_avg_ratio(self):
        candles = {"1h": make_candles_1h(168)}
        ctx = _volume_context(candles)
        assert "vs_7d_avg_ratio" in ctx
        # Should be > 1 since volume is increasing
        assert ctx["vs_7d_avg_ratio"] > 1.0

    def test_empty_candles(self):
        ctx = _volume_context({})
        assert ctx == {}


# ---------------------------------------------------------------------------
# Level context
# ---------------------------------------------------------------------------

class TestLevelContext:
    def test_24h_high_low(self):
        candles_5m = make_candles_5m(288)
        ctx = _level_context({"5m": candles_5m})
        # Highs: c + 50, so max at last candle = 67870 + 50 = 67920
        assert ctx["high_24h"] == 67920.0
        # Lows: c - 50, so min at first candle = 65000 - 50 = 64950
        assert ctx["low_24h"] == 64950.0

    def test_7d_high_low(self):
        ctx = _level_context({"1h": make_candles_1h(168)})
        assert "high_7d" in ctx
        assert "low_7d" in ctx
        assert ctx["high_7d"] > ctx["low_7d"]

    def test_30d_high_low(self):
        ctx = _level_context({"1d": make_candles_1d(90)})
        assert "high_30d" in ctx
        assert "low_30d" in ctx
        assert ctx["high_30d"] > ctx["low_30d"]


# ---------------------------------------------------------------------------
# Funding context
# ---------------------------------------------------------------------------

class TestFundingContext:
    def test_current_rate(self):
        funding = make_funding(90)
        ctx = _funding_context(funding)
        expected = 0.0001 + 89 * 0.00001
        assert abs(ctx["current"] - expected) < 1e-10

    def test_7d_average(self):
        funding = make_funding(90)
        ctx = _funding_context(funding)
        # Last 21 entries: rates from index 69 to 89
        rates_7d = [0.0001 + i * 0.00001 for i in range(69, 90)]
        expected = np.mean(rates_7d)
        assert abs(ctx["avg_7d"] - expected) < 1e-10

    def test_percentile(self):
        funding = make_funding(90)
        ctx = _funding_context(funding)
        # Last rate is the highest, so percentile should be ~99%
        assert ctx["percentile_90d"] > 95

    def test_same_sign_streak_all_positive(self):
        funding = make_funding(90, base_rate=0.0001)
        ctx = _funding_context(funding)
        # All rates are positive → streak = 90
        assert ctx["same_sign_streak"] == 90

    def test_same_sign_streak_mixed(self):
        funding = [
            {"ts": i, "rate": -0.001 if i < 5 else 0.001}
            for i in range(10)
        ]
        ctx = _funding_context(funding)
        assert ctx["same_sign_streak"] == 5

    def test_trend(self):
        funding = make_funding(90)
        ctx = _funding_context(funding)
        # Rates are increasing, so trend should be positive
        assert ctx["trend_pct"] > 0

    def test_empty_funding(self):
        ctx = _funding_context([])
        assert ctx == {}


# ---------------------------------------------------------------------------
# Open interest context
# ---------------------------------------------------------------------------

class TestOIContext:
    def test_current_value(self):
        oi = make_oi(720)
        ctx = _oi_context(oi)
        expected = 5_000_000_000.0 + 719 * 1_000_000.0
        assert abs(ctx["current"] - expected) < 0.01

    def test_24h_change(self):
        oi = make_oi(720)
        ctx = _oi_context(oi)
        assert "change_24h_pct" in ctx
        assert ctx["change_24h_pct"] > 0  # OI is increasing

    def test_7d_change(self):
        oi = make_oi(720)
        ctx = _oi_context(oi)
        assert "change_7d_pct" in ctx
        assert ctx["change_7d_pct"] > 0

    def test_30d_change(self):
        oi = make_oi(720)
        ctx = _oi_context(oi)
        assert "change_30d_pct" in ctx
        assert ctx["change_30d_pct"] > 0

    def test_7d_high_low(self):
        oi = make_oi(720)
        ctx = _oi_context(oi)
        assert ctx["high_7d"] > ctx["low_7d"]

    def test_empty_oi(self):
        ctx = _oi_context([])
        assert ctx == {}


# ---------------------------------------------------------------------------
# Market / index context
# ---------------------------------------------------------------------------

class TestMarketContext:
    def test_index_current(self):
        entries = make_index(90, base_value=50.0)
        ctx = _index_context(entries)
        expected = 50.0 + 89 * 0.5
        assert abs(ctx["current"] - expected) < 0.01

    def test_index_averages(self):
        entries = make_index(90, base_value=50.0)
        ctx = _index_context(entries)
        assert "avg_7d" in ctx
        assert "avg_30d" in ctx

    def test_index_changes(self):
        entries = make_index(90, base_value=50.0)
        ctx = _index_context(entries)
        assert ctx["change_7d"] > 0  # Rising trend
        assert ctx["change_30d"] > 0

    def test_index_trend_rising(self):
        entries = make_index(90, base_value=50.0)
        ctx = _index_context(entries)
        assert ctx["trend"] == "rising"

    def test_index_trend_falling(self):
        entries = [{"ts": i, "value": 100.0 - i * 0.5} for i in range(90)]
        ctx = _index_context(entries)
        assert ctx["trend"] == "falling"

    def test_empty_index(self):
        ctx = _index_context([])
        assert ctx == {}


# ---------------------------------------------------------------------------
# Full pipeline
# ---------------------------------------------------------------------------

class TestFullPipeline:
    def test_build_context_structure(self):
        data = {
            "assets": ["BTCUSDT", "ETHUSDT"],
            "candles": {
                "BTCUSDT": {
                    "5m": make_candles_5m(288),
                    "1h": make_candles_1h(168),
                    "1d": make_candles_1d(90),
                },
                "ETHUSDT": {
                    "5m": make_candles_5m(288, base=3500),
                    "1h": make_candles_1h(168, base=3500),
                    "1d": make_candles_1d(90, base=3500),
                },
            },
            "funding_rates": {
                "BTCUSDT": make_funding(90),
                "ETHUSDT": make_funding(90, base_rate=0.0002),
            },
            "open_interest": {
                "BTCUSDT": make_oi(720),
                "ETHUSDT": make_oi(720, base_value=2_000_000_000.0),
            },
            "indices": {
                "fear_greed": make_index(90, base_value=50.0),
                "btc_dominance": make_index(90, base_value=55.0),
                "eth_dominance": make_index(90, base_value=18.0),
                "total_market_cap": make_index(90, base_value=2_500_000_000_000.0),
            },
        }

        result = build_context(data)

        # Structure checks
        assert "assets" in result
        assert "market" in result
        assert "BTCUSDT" in result["assets"]
        assert "ETHUSDT" in result["assets"]

        # Per-asset sections
        btc = result["assets"]["BTCUSDT"]
        assert all(k in btc for k in ("price", "volume", "levels", "funding", "open_interest"))
        assert btc["price"]["current"] > 0
        assert btc["funding"]["current"] is not None

        # Market sections
        mkt = result["market"]
        assert all(k in mkt for k in ("fear_greed", "btc_dominance", "eth_dominance", "total_market_cap"))
        assert mkt["fear_greed"]["current"] > 0

    def test_missing_data_graceful(self):
        data = {
            "assets": ["BTCUSDT"],
            "candles": {"BTCUSDT": {}},
            "funding_rates": {},
            "open_interest": {},
            "indices": {},
        }
        result = build_context(data)
        assert "BTCUSDT" in result["assets"]
        assert result["market"] == {}


# ---------------------------------------------------------------------------
# Utility
# ---------------------------------------------------------------------------

class TestUtilities:
    def test_pct_change(self):
        assert abs(_pct_change(100.0, 110.0) - 10.0) < 0.001
        assert abs(_pct_change(100.0, 90.0) - (-10.0)) < 0.001
        assert _pct_change(0.0, 100.0) is None

    def test_pct_change_negative_base(self):
        # Negative base: -0.001 to -0.002
        result = _pct_change(-0.001, -0.002)
        assert abs(result - (-100.0)) < 0.001
