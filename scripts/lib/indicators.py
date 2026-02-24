"""Technical indicator computation.

Computes a comprehensive set of indicators from candle data using the `ta` library.
Returns the last N periods as time-series snapshots for each symbol/timeframe.

Indicator set:
  Trend:      SMA (20/50/200), EMA (9/21/50/200), MACD (12/26/9), ADX/DI+/DI-,
              Ichimoku (tenkan/kijun/senkou_a/senkou_b/chikou)
  Momentum:   RSI (14), Stochastic RSI (k/d), CCI (20), Williams %R, ROC (12), MFI (14)
  Volatility: Bollinger Bands (20/2σ), ATR (14), Keltner Channels, historical volatility (20), bandwidth
  Volume:     OBV, VWAP (intraday only), volume SMA (20), accumulation/distribution, Chaikin money flow (20)
  Structure:  Pivot Points (classic), swing high/low
"""

import math

import numpy as np
import pandas as pd
import ta

OUTPUT_PERIODS = 20


def compute_all_indicators(data: dict) -> dict:
    """Compute all indicators for all assets across all timeframes.

    Args:
        data: Input from Rust caller.
            {
              "assets": ["BTCUSDT", ...],
              "candles": {
                "BTCUSDT": {
                  "5m": [{"ts": ..., "o": ..., "h": ..., "l": ..., "c": ..., "v": ...}, ...],
                  "1h": [...],
                  "1d": [...]
                }
              }
            }

    Returns:
        Dict of asset -> timeframe -> {"periods": [snapshot, ...]}
    """
    assets = data["assets"]
    candles = data["candles"]
    result = {}

    for asset in assets:
        asset_data = candles.get(asset, {})
        result[asset] = {}

        for timeframe, candle_list in asset_data.items():
            if not candle_list or len(candle_list) < 2:
                continue

            df = _candles_to_dataframe(candle_list)
            periods = _compute_for_timeframe(df, timeframe)
            result[asset][timeframe] = {"periods": periods}

    return result


def _candles_to_dataframe(candle_list: list[dict]) -> pd.DataFrame:
    """Convert list of candle dicts to a pandas DataFrame."""
    df = pd.DataFrame(candle_list)
    df = df.rename(columns={"ts": "timestamp", "o": "open", "h": "high", "l": "low", "c": "close", "v": "volume"})
    df = df.sort_values("timestamp").reset_index(drop=True)
    return df


def _compute_for_timeframe(df: pd.DataFrame, timeframe: str) -> list[dict]:
    """Compute all indicators and return the last OUTPUT_PERIODS as snapshots."""
    is_intraday = timeframe in ("5m", "1h")
    n = len(df)
    high = df["high"]
    low = df["low"]
    close = df["close"]
    volume = df["volume"]

    indicators = {}

    # --- Trend ---
    for period in (20, 50, 200):
        if n >= period:
            indicators[f"sma_{period}"] = ta.trend.sma_indicator(close, window=period)

    for period in (9, 21, 50, 200):
        if n >= period:
            indicators[f"ema_{period}"] = ta.trend.ema_indicator(close, window=period)

    if n >= 26:
        macd_obj = ta.trend.MACD(close, window_slow=26, window_fast=12, window_sign=9)
        indicators["macd_line"] = macd_obj.macd()
        indicators["macd_signal"] = macd_obj.macd_signal()
        indicators["macd_histogram"] = macd_obj.macd_diff()

    if n >= 28:  # ADX needs window*2 data points
        adx_obj = ta.trend.ADXIndicator(high, low, close, window=14)
        indicators["adx"] = adx_obj.adx()
        indicators["adx_di_plus"] = adx_obj.adx_pos()
        indicators["adx_di_minus"] = adx_obj.adx_neg()

    if n >= 52:
        ichi_obj = ta.trend.IchimokuIndicator(high, low, window1=9, window2=26, window3=52)
        indicators["ichimoku_tenkan"] = ichi_obj.ichimoku_conversion_line()
        indicators["ichimoku_kijun"] = ichi_obj.ichimoku_base_line()
        indicators["ichimoku_senkou_a"] = ichi_obj.ichimoku_a()
        indicators["ichimoku_senkou_b"] = ichi_obj.ichimoku_b()
        # Chikou reference: close from 26 periods ago (what chikou span is compared against)
        indicators["ichimoku_chikou_ref"] = close.shift(26)

    # --- Momentum ---
    if n >= 14:
        indicators["rsi_14"] = ta.momentum.rsi(close, window=14)

        stoch_rsi = ta.momentum.StochRSIIndicator(close, window=14, smooth1=3, smooth2=3)
        indicators["stochastic_rsi_k"] = stoch_rsi.stochrsi_k()
        indicators["stochastic_rsi_d"] = stoch_rsi.stochrsi_d()

        indicators["williams_r"] = ta.momentum.williams_r(high, low, close, lbp=14)

        indicators["mfi_14"] = ta.volume.money_flow_index(high, low, close, volume, window=14)

    if n >= 20:
        indicators["cci_20"] = ta.trend.cci(high, low, close, window=20)

    if n >= 12:
        indicators["roc_12"] = ta.momentum.roc(close, window=12)

    # --- Volatility ---
    if n >= 20:
        bb_obj = ta.volatility.BollingerBands(close, window=20, window_dev=2)
        indicators["bollinger_upper"] = bb_obj.bollinger_hband()
        indicators["bollinger_mid"] = bb_obj.bollinger_mavg()
        indicators["bollinger_lower"] = bb_obj.bollinger_lband()
        indicators["bollinger_bandwidth"] = bb_obj.bollinger_wband()

    if n >= 14:
        indicators["atr_14"] = ta.volatility.average_true_range(high, low, close, window=14)

    if n >= 20:
        kc_obj = ta.volatility.KeltnerChannel(high, low, close, window=20, window_atr=10)
        indicators["keltner_upper"] = kc_obj.keltner_channel_hband()
        indicators["keltner_mid"] = kc_obj.keltner_channel_mband()
        indicators["keltner_lower"] = kc_obj.keltner_channel_lband()

    # Historical volatility: 20-period annualized std dev of log returns
    if n >= 21:
        log_returns = np.log(close / close.shift(1))
        periods_per_year = {"5m": 105120, "1h": 8760, "1d": 365}[timeframe]
        indicators["historical_volatility_20"] = log_returns.rolling(window=20).std() * np.sqrt(periods_per_year)

    # --- Volume ---
    if n >= 2:
        indicators["obv"] = ta.volume.on_balance_volume(close, volume)

    if is_intraday and n >= 14:
        indicators["vwap"] = ta.volume.volume_weighted_average_price(high, low, close, volume)

    if n >= 20:
        indicators["volume_sma_20"] = ta.trend.sma_indicator(volume, window=20)
        indicators["volume_ratio"] = volume / ta.trend.sma_indicator(volume, window=20)

    indicators["accumulation_distribution"] = ta.volume.acc_dist_index(high, low, close, volume)

    if n >= 20:
        indicators["chaikin_money_flow_20"] = ta.volume.chaikin_money_flow(high, low, close, volume, window=20)

    # --- Structure ---
    _add_pivot_points(df, indicators)
    _add_swing_points(df, indicators)

    # --- Build output snapshots ---
    n = len(df)
    start = max(0, n - OUTPUT_PERIODS)
    periods = []

    for i in range(start, n):
        snapshot = {
            "ts": int(df.at[i, "timestamp"]),
            "candle": {
                "o": _clean(df.at[i, "open"]),
                "h": _clean(df.at[i, "high"]),
                "l": _clean(df.at[i, "low"]),
                "c": _clean(df.at[i, "close"]),
                "v": _clean(df.at[i, "volume"]),
            },
        }

        for key, series in indicators.items():
            if isinstance(series, pd.Series):
                snapshot[key] = _clean(series.iat[i]) if i < len(series) else None
            else:
                # Scalar value (pivot points, swing points) — same for all periods
                snapshot[key] = _clean(series)

        periods.append(snapshot)

    return periods


def _add_pivot_points(df: pd.DataFrame, indicators: dict):
    """Compute classic pivot points from the previous period's high/low/close."""
    if len(df) < 2:
        return

    # Use the previous completed candle for pivot calculation
    prev = df.iloc[-2]
    h, l, c = prev["high"], prev["low"], prev["close"]

    pivot = (h + l + c) / 3.0
    indicators["pivot"] = pivot
    indicators["pivot_r1"] = 2 * pivot - l
    indicators["pivot_s1"] = 2 * pivot - h
    indicators["pivot_r2"] = pivot + (h - l)
    indicators["pivot_s2"] = pivot - (h - l)


def _add_swing_points(df: pd.DataFrame, indicators: dict, lookback: int = 5):
    """Identify the most recent swing high and swing low within a lookback window.

    A swing high is a candle whose high is greater than the highs of `lookback`
    candles on both sides. Same logic inverted for swing low.
    """
    n = len(df)
    swing_high = None
    swing_low = None

    # Search backwards from the end to find the most recent swing points
    # We need at least `lookback` candles on each side
    for i in range(n - lookback - 1, lookback - 1, -1):
        if swing_high is not None and swing_low is not None:
            break

        if swing_high is None:
            is_swing_high = True
            for j in range(1, lookback + 1):
                if df.at[i - j, "high"] >= df.at[i, "high"] or df.at[i + j, "high"] >= df.at[i, "high"]:
                    is_swing_high = False
                    break
            if is_swing_high:
                swing_high = df.at[i, "high"]

        if swing_low is None:
            is_swing_low = True
            for j in range(1, lookback + 1):
                if df.at[i - j, "low"] <= df.at[i, "low"] or df.at[i + j, "low"] <= df.at[i, "low"]:
                    is_swing_low = False
                    break
            if is_swing_low:
                swing_low = df.at[i, "low"]

    indicators["swing_high"] = swing_high
    indicators["swing_low"] = swing_low


def _clean(value) -> float | None:
    """Convert a value to a clean float, returning None for NaN/inf."""
    if value is None:
        return None
    if isinstance(value, (int, float)):
        if math.isnan(value) or math.isinf(value):
            return None
        return round(float(value), 8)
    try:
        f = float(value)
        if math.isnan(f) or math.isinf(f):
            return None
        return round(f, 8)
    except (TypeError, ValueError):
        return None
