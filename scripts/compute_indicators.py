"""Compute technical indicators for live evaluation.

Contract:
  - Receives JSON on stdin: asset list + candle data per manifest.toml requirements
  - Returns JSON on stdout: flat key-value indicators per asset
  - Timeout: 30s (enforced by Rust caller)

Input shape:
  {
    "assets": ["BTCUSDT", ...],
    "candles_5m": { "BTCUSDT": [{"open_time": ..., "o": ..., "h": ..., "l": ..., "c": ..., "v": ...}] },
    "candles_1h": { ... },
    "candles_1d": { ... }
  }

Output shape:
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
    ...
  }
"""

import json
import sys

from lib.indicators import compute_all_indicators


def main():
    data = json.load(sys.stdin)
    result = compute_all_indicators(data)
    json.dump(result, sys.stdout)


if __name__ == "__main__":
    main()
