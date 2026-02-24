"""Compute technical indicators for all assets and timeframes.

Contract:
  - Receives JSON on stdin: asset list + candle data per manifest.toml requirements
  - Returns JSON on stdout: per-asset, per-timeframe indicator windows (20 periods each)
  - Timeout: 30s (enforced by Rust caller)

Input shape:
  {
    "assets": ["BTCUSDT", "ETHUSDT", ...],
    "candles": {
      "BTCUSDT": {
        "5m": [{"ts": ..., "o": ..., "h": ..., "l": ..., "c": ..., "v": ...}, ...],
        "1h": [...],
        "1d": [...]
      },
      ...
    }
  }

Output shape:
  {
    "BTCUSDT": {
      "5m": {
        "periods": [
          {
            "ts": 1708300800000,
            "candle": {"o": 65012.4, "h": 65189.0, "l": 64901.3, "c": 65050.7, "v": 1842.3},
            "sma_20": 64803.1,
            "sma_50": 64210.5,
            "sma_200": 62100.0,
            "ema_9": 65020.3,
            ...
          },
          ...  (20 periods)
        ]
      },
      "1h": { ... },
      "1d": { ... }
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
