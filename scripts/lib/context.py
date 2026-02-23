"""Context assembly for LLM report generation.

Assembles pre-computed numerical picture from raw market data
into structured context for each report type. Output feeds
directly into the LLM prompt as Layer 3 per-call context.
"""


def build_context(data: dict) -> dict:
    """Build LLM context from market data.

    Args:
        data: Input from Rust with full market data per manifest requirements.

    Returns:
        Per-asset context blocks plus cross-market data.
    """
    return {}
