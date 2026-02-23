"""Build pre-computed numerical context for LLM reports.

Contract:
  - Receives JSON on stdin: full market data per manifest.toml requirements
  - Returns JSON on stdout: rich per-asset context blocks + cross-market data
  - Timeout: 60s (enforced by Rust caller)

Input shape:
  {
    "assets": ["BTCUSDT", ...],
    "candles_5m": { ... },
    "candles_1h": { ... },
    "candles_1d": { ... },
    "funding_rates": { ... },
    "open_interest": { ... },
    "indices": { ... }
  }

Output shape:
  Per-asset context blocks with narrative-ready numerical context,
  plus cross-market data. Feeds directly into LLM prompt as
  Layer 3 per-call context.
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
