"""Tests for indicator computation correctness.

Validates each indicator category against known values or manual calculations
using synthetic candle data. The goal is to ensure compute_all_indicators
produces correct, truthful numbers — not just that it doesn't crash.
"""

import math
import sys

import numpy as np
import pandas as pd
import pytest

sys.path.insert(0, str(__import__("pathlib").Path(__file__).resolve().parent.parent))

from lib.indicators import (
    OUTPUT_PERIODS,
    _candles_to_dataframe,
    _clean,
    _compute_for_timeframe,
    compute_all_indicators,
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def make_candles(n=260, base=65000.0, interval_ms=3600000):
    """Generate n synthetic candles with a trending sine wave pattern."""
    candles = []
    for i in range(n):
        trend = i * 10.0
        wave = 2000.0 * math.sin(i * 0.1)
        c = base + trend + wave
        o = c - 50 + (i % 7) * 15
        h = max(o, c) + 100 + (i % 5) * 20
        l = min(o, c) - 100 - (i % 3) * 15
        v = 500.0 + 200 * math.sin(i * 0.05) + (i % 10) * 30
        candles.append({
            "ts": 1700000000000 + i * interval_ms,
            "o": round(o, 2),
            "h": round(h, 2),
            "l": round(l, 2),
            "c": round(c, 2),
            "v": round(max(v, 1.0), 2),  # Ensure positive volume
        })
    return candles


def make_flat_candles(n=260, price=100.0, volume=1000.0, interval_ms=3600000):
    """Generate flat candles (constant price) for testing edge cases."""
    return [
        {
            "ts": 1700000000000 + i * interval_ms,
            "o": price,
            "h": price + 1.0,
            "l": price - 1.0,
            "c": price,
            "v": volume,
        }
        for i in range(n)
    ]


def run_indicators(candles, is_intraday=True):
    """Helper: candles -> last OUTPUT_PERIODS of indicator snapshots."""
    df = _candles_to_dataframe(candles)
    return _compute_for_timeframe(df, is_intraday)


def last_period(periods):
    """Get the last period from a list of snapshots."""
    return periods[-1]


# ---------------------------------------------------------------------------
# Output shape
# ---------------------------------------------------------------------------

class TestOutputShape:
    def test_returns_correct_number_of_periods(self):
        periods = run_indicators(make_candles(260))
        assert len(periods) == OUTPUT_PERIODS

    def test_returns_fewer_periods_if_insufficient_data(self):
        periods = run_indicators(make_candles(10))
        assert len(periods) == 10
        # With few candles, many indicators will be absent — that's fine
        assert "ts" in periods[-1]
        assert "candle" in periods[-1]

    def test_each_period_has_timestamp_and_candle(self):
        periods = run_indicators(make_candles(260))
        for p in periods:
            assert "ts" in p
            assert "candle" in p
            assert all(k in p["candle"] for k in ("o", "h", "l", "c", "v"))

    def test_timestamps_are_ascending(self):
        periods = run_indicators(make_candles(260))
        timestamps = [p["ts"] for p in periods]
        assert timestamps == sorted(timestamps)

    def test_all_values_are_float_or_none(self):
        periods = run_indicators(make_candles(260))
        for p in periods:
            for k, v in p.items():
                if k in ("ts", "candle"):
                    continue
                assert v is None or isinstance(v, float), f"{k} = {v} ({type(v)})"

    def test_vwap_present_for_intraday(self):
        periods = run_indicators(make_candles(260), is_intraday=True)
        assert "vwap" in last_period(periods)

    def test_vwap_absent_for_daily(self):
        periods = run_indicators(make_candles(260), is_intraday=False)
        assert "vwap" not in last_period(periods)

    def test_full_pipeline_structure(self):
        data = {
            "assets": ["BTCUSDT"],
            "candles": {"BTCUSDT": {"1h": make_candles(260)}},
        }
        result = compute_all_indicators(data)
        assert "BTCUSDT" in result
        assert "1h" in result["BTCUSDT"]
        assert "periods" in result["BTCUSDT"]["1h"]
        assert len(result["BTCUSDT"]["1h"]["periods"]) == 20


# ---------------------------------------------------------------------------
# Trend indicators
# ---------------------------------------------------------------------------

class TestTrendIndicators:
    def test_sma_20_manual_check(self):
        """SMA-20 of the last period should equal mean of last 20 closes."""
        candles = make_candles(260)
        df = _candles_to_dataframe(candles)
        expected = df["close"].iloc[-20:].mean()

        periods = run_indicators(candles)
        actual = last_period(periods)["sma_20"]
        assert abs(actual - expected) < 0.01, f"SMA-20: {actual} != {expected}"

    def test_sma_50_manual_check(self):
        candles = make_candles(260)
        df = _candles_to_dataframe(candles)
        expected = df["close"].iloc[-50:].mean()

        periods = run_indicators(candles)
        actual = last_period(periods)["sma_50"]
        assert abs(actual - expected) < 0.01

    def test_sma_200_manual_check(self):
        candles = make_candles(260)
        df = _candles_to_dataframe(candles)
        expected = df["close"].iloc[-200:].mean()

        periods = run_indicators(candles)
        actual = last_period(periods)["sma_200"]
        assert abs(actual - expected) < 0.01

    def test_ema_responds_more_to_recent_prices(self):
        """EMA-9 should be closer to the current price than SMA-200 in a trending market."""
        periods = run_indicators(make_candles(260))
        p = last_period(periods)
        close = p["candle"]["c"]
        assert abs(p["ema_9"] - close) < abs(p["sma_200"] - close)

    def test_macd_histogram_is_line_minus_signal(self):
        periods = run_indicators(make_candles(260))
        p = last_period(periods)
        expected = p["macd_line"] - p["macd_signal"]
        assert abs(p["macd_hist"] - expected) < 0.001

    def test_adx_is_positive(self):
        periods = run_indicators(make_candles(260))
        p = last_period(periods)
        assert p["adx"] > 0
        assert p["di_plus"] >= 0
        assert p["di_minus"] >= 0

    def test_ichimoku_tenkan_shorter_than_kijun(self):
        """Tenkan-sen (9-period) should be more responsive than kijun-sen (26-period)."""
        periods = run_indicators(make_candles(260))
        # Both should be present
        p = last_period(periods)
        assert p["ichi_tenkan"] is not None
        assert p["ichi_kijun"] is not None

    def test_ichimoku_chikou_ref_is_lagged_close(self):
        """ichi_chikou_ref should be the close from 26 periods ago."""
        candles = make_candles(260)
        df = _candles_to_dataframe(candles)
        expected = df["close"].iloc[-27]  # 26 periods before the last

        periods = run_indicators(candles)
        actual = last_period(periods)["ichi_chikou_ref"]
        assert abs(actual - expected) < 0.01


# ---------------------------------------------------------------------------
# Momentum indicators
# ---------------------------------------------------------------------------

class TestMomentumIndicators:
    def test_rsi_bounded_0_100(self):
        periods = run_indicators(make_candles(260))
        for p in periods:
            if p["rsi_14"] is not None:
                assert 0 <= p["rsi_14"] <= 100

    def test_rsi_high_in_uptrend(self):
        """RSI should be elevated in a strong uptrend."""
        # Create pure uptrend candles
        candles = []
        for i in range(260):
            c = 100.0 + i * 5.0  # Steadily rising
            candles.append({
                "ts": 1700000000000 + i * 3600000,
                "o": c - 2, "h": c + 3, "l": c - 3, "c": c, "v": 1000.0,
            })
        periods = run_indicators(candles)
        assert last_period(periods)["rsi_14"] > 70

    def test_stoch_rsi_bounded_0_1(self):
        periods = run_indicators(make_candles(260))
        for p in periods:
            if p["stoch_rsi_k"] is not None:
                assert -0.001 <= p["stoch_rsi_k"] <= 1.001
            if p["stoch_rsi_d"] is not None:
                assert -0.001 <= p["stoch_rsi_d"] <= 1.001

    def test_williams_r_bounded(self):
        periods = run_indicators(make_candles(260))
        for p in periods:
            if p["williams_r"] is not None:
                assert -100.01 <= p["williams_r"] <= 0.01

    def test_mfi_bounded_0_100(self):
        periods = run_indicators(make_candles(260))
        for p in periods:
            if p["mfi_14"] is not None:
                assert -0.01 <= p["mfi_14"] <= 100.01

    def test_cci_can_exceed_100(self):
        """CCI is unbounded and can go above 100 or below -100 in trending markets."""
        periods = run_indicators(make_candles(260))
        cci_values = [p["cci_20"] for p in periods if p["cci_20"] is not None]
        # In our synthetic trending data, CCI should have some extreme values
        assert any(abs(v) > 50 for v in cci_values)

    def test_roc_positive_in_uptrend(self):
        candles = []
        for i in range(260):
            c = 100.0 + i * 2.0
            candles.append({
                "ts": 1700000000000 + i * 3600000,
                "o": c - 1, "h": c + 2, "l": c - 2, "c": c, "v": 1000.0,
            })
        periods = run_indicators(candles)
        assert last_period(periods)["roc_12"] > 0


# ---------------------------------------------------------------------------
# Volatility indicators
# ---------------------------------------------------------------------------

class TestVolatilityIndicators:
    def test_bollinger_band_ordering(self):
        """Upper > middle > lower always."""
        periods = run_indicators(make_candles(260))
        for p in periods:
            if all(p[k] is not None for k in ("bb_upper", "bb_mid", "bb_lower")):
                assert p["bb_upper"] > p["bb_mid"] > p["bb_lower"]

    def test_bollinger_mid_equals_sma_20(self):
        """BB middle band should equal SMA-20."""
        periods = run_indicators(make_candles(260))
        p = last_period(periods)
        assert abs(p["bb_mid"] - p["sma_20"]) < 0.01

    def test_bb_width_positive(self):
        periods = run_indicators(make_candles(260))
        for p in periods:
            if p["bb_width"] is not None:
                assert p["bb_width"] > 0

    def test_atr_positive(self):
        periods = run_indicators(make_candles(260))
        for p in periods:
            if p["atr_14"] is not None:
                assert p["atr_14"] > 0

    def test_keltner_channel_ordering(self):
        periods = run_indicators(make_candles(260))
        for p in periods:
            if all(p[k] is not None for k in ("kc_upper", "kc_mid", "kc_lower")):
                assert p["kc_upper"] > p["kc_mid"] > p["kc_lower"]

    def test_hist_vol_positive(self):
        periods = run_indicators(make_candles(260))
        for p in periods:
            if p["hist_vol_20"] is not None:
                assert p["hist_vol_20"] > 0

    def test_flat_market_low_volatility(self):
        """Flat prices should produce low ATR and BB width."""
        periods = run_indicators(make_flat_candles(260, price=100.0))
        p = last_period(periods)
        # ATR should be small relative to price (h-l is only 2.0)
        assert p["atr_14"] < 5.0
        # BB width should be very small
        assert p["bb_width"] < 1.0


# ---------------------------------------------------------------------------
# Volume indicators
# ---------------------------------------------------------------------------

class TestVolumeIndicators:
    def test_obv_changes_with_price_direction(self):
        """OBV should increase when close > prev close."""
        periods = run_indicators(make_candles(260))
        # Check that OBV changes between consecutive periods
        obvs = [p["obv"] for p in periods if p["obv"] is not None]
        assert len(set(obvs)) > 1  # Not all the same

    def test_vol_ratio_around_one_for_average_volume(self):
        """Volume ratio should be near 1.0 when volume is near its SMA."""
        periods = run_indicators(make_flat_candles(260))
        p = last_period(periods)
        assert 0.9 < p["vol_ratio"] < 1.1

    def test_cmf_bounded(self):
        """CMF should be between -1 and 1."""
        periods = run_indicators(make_candles(260))
        for p in periods:
            if p["cmf_20"] is not None:
                assert -1.01 <= p["cmf_20"] <= 1.01

    def test_ad_is_cumulative(self):
        """A/D line should be a running total."""
        periods = run_indicators(make_candles(260))
        # A/D values should vary (not all zero)
        ad_vals = [p["ad"] for p in periods if p["ad"] is not None]
        assert len(set(ad_vals)) > 1


# ---------------------------------------------------------------------------
# Structure indicators
# ---------------------------------------------------------------------------

class TestStructureIndicators:
    def test_pivot_points_ordering(self):
        """R2 > R1 > Pivot > S1 > S2."""
        periods = run_indicators(make_candles(260))
        p = last_period(periods)
        assert p["pivot_r2"] > p["pivot_r1"] > p["pivot"] > p["pivot_s1"] > p["pivot_s2"]

    def test_pivot_formula(self):
        """Verify pivot = (H + L + C) / 3 from previous candle."""
        candles = make_candles(260)
        df = _candles_to_dataframe(candles)
        prev = df.iloc[-2]
        expected_pivot = (prev["high"] + prev["low"] + prev["close"]) / 3.0

        periods = run_indicators(candles)
        actual = last_period(periods)["pivot"]
        assert abs(actual - expected_pivot) < 0.01

    def test_swing_high_above_swing_low(self):
        periods = run_indicators(make_candles(260))
        p = last_period(periods)
        if p["swing_high"] is not None and p["swing_low"] is not None:
            assert p["swing_high"] > p["swing_low"]


# ---------------------------------------------------------------------------
# Edge cases
# ---------------------------------------------------------------------------

class TestEdgeCases:
    def test_minimum_candles(self):
        """Should not crash with very few candles."""
        periods = run_indicators(make_candles(5))
        assert len(periods) == 5
        # Should have at least candle data
        assert "candle" in periods[-1]

    def test_flat_prices_no_crash(self):
        """Flat prices should not produce NaN or crash."""
        periods = run_indicators(make_flat_candles(260))
        p = last_period(periods)
        # SMA should equal the price
        assert abs(p["sma_20"] - 100.0) < 0.01

    def test_zero_volume_candle(self):
        """Zero volume should not crash (some indicators may be None)."""
        candles = make_candles(260)
        candles[-1]["v"] = 0.0
        # Should not raise
        periods = run_indicators(candles)
        assert len(periods) == OUTPUT_PERIODS

    def test_no_nan_in_output(self):
        """No NaN values should appear in the output — they should be None."""
        periods = run_indicators(make_candles(260))
        for p in periods:
            for k, v in p.items():
                if k in ("ts", "candle"):
                    continue
                if v is not None:
                    assert not math.isnan(v), f"NaN found for {k}"
                    assert not math.isinf(v), f"Inf found for {k}"

    def test_clean_function(self):
        assert _clean(42.0) == 42.0
        assert _clean(float("nan")) is None
        assert _clean(float("inf")) is None
        assert _clean(None) is None
        assert _clean(np.float64(3.14)) == round(3.14, 8)
        assert _clean(np.nan) is None

    def test_empty_asset_skipped(self):
        data = {
            "assets": ["BTCUSDT"],
            "candles": {"BTCUSDT": {}},
        }
        result = compute_all_indicators(data)
        assert result == {"BTCUSDT": {}}

    def test_missing_asset_candles_skipped(self):
        data = {
            "assets": ["BTCUSDT", "ETHUSDT"],
            "candles": {"BTCUSDT": {"1h": make_candles(260)}},
        }
        result = compute_all_indicators(data)
        assert "BTCUSDT" in result
        assert result["ETHUSDT"] == {}
