"""Build pre-computed numerical context for LLM reports.

Contract:
  - Receives JSON on stdin: full market data per manifest.toml requirements
  - Returns JSON on stdout: per-asset context + cross-market context
  - Timeout: 60s (enforced by Rust caller)

Input shape:
  {
    "assets": ["BTCUSDT", ...],
    "candles": {
      "BTCUSDT": {
        "5m": [{"ts": ..., "o": ..., "h": ..., "l": ..., "c": ..., "v": ...}, ...],
        "1h": [...],
        "1d": [...]
      }
    },
    "funding_rates": {
      "BTCUSDT": [{"ts": ..., "rate": ...}, ...]
    },
    "open_interest": {
      "BTCUSDT": [{"ts": ..., "value": ...}, ...]
    },
    "indices": {
      "fear_greed": [{"ts": ..., "value": ...}, ...],
      "btc_dominance": [...],
      "eth_dominance": [...],
      "total_market_cap": [...]
    }
  }

Output shape:
  {
    "assets": {
      "BTCUSDT": {
        "price": { "current": ..., "change_24h_pct": ..., ... },
        "volume": { ... },
        "funding": { ... },
        "open_interest": { ... },
        "levels": { ... }
      }
    },
    "market": {
      "fear_greed": { ... },
      "btc_dominance": { ... },
      "eth_dominance": { ... },
      "total_market_cap": { ... }
    }
  }
"""

import json
import sys

from lib.context import build_context


def main():
    data = json.load(sys.stdin)
    result = build_context(data)
    json.dump(result, sys.stdout)


if __name__ == "__main__":
    main()
