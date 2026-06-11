"""Money-market / markets-balance-sheet products: the spread-to-short
floaters that the core model excludes -- interest-earning deposits with
banks (reserves), fed funds sold / reverse repo, trading-inventory carry,
fed funds purchased / repo, short-term borrowings, trading liabilities.

These are the SIMPLEST products in the engine: no optionality, no
behavior, no schedule. Each position accrues balance * (short_t + spread)
monthly, with the balance held constant over the horizon (rollover
assumption: matched books roll at prevailing rates -- the accounting
reality of an overnight/term-matched markets balance sheet). They exist
so the model's NIM denominator and numerator can include the low-spread
balances that dilute a reported bank NIM -- the largest composition
difference between a "core" model and a G-SIB headline (WFC 1Q26: ~$570B
of these assets at ~3.6% vs repo funding at ~3.7%).

Valuation: near-zero duration by construction (rate resets monthly with
the short path). dv01 reported as the one-month reset lag effect only --
do not expect KRDs; that is the point of the product.

Schema (frame): id, balance, side ("asset"|"liability"),
spread_bp (to the simulated short rate), category (free text label).
"""
from __future__ import annotations

import numpy as np
import polars as pl

MM_COLS = {"id", "balance", "side", "spread_bp", "category"}


class MMDeck:
    def __init__(self, book: pl.DataFrame):
        missing = MM_COLS - set(book.columns)
        if missing:
            raise ValueError(f"mm book missing columns: {missing}")
        self.bal = book["balance"].to_numpy().astype(np.float64)
        self.spr = book["spread_bp"].to_numpy().astype(np.float64) * 1e-4
        self.sign = np.where(
            np.array(book["side"].to_list()) == "asset", 1.0, -1.0)
        self.n = len(book)


def mm_income(deck: MMDeck, short: np.ndarray, horizon: int
              ) -> tuple[np.ndarray, np.ndarray]:
    """(asset_income[m], liability_expense[m]) expected monthly accruals,
    $; rate floored at 0 per position (negative-rate pass-through is a
    policy choice -- floor matches USD convention)."""
    P = short.shape[0]
    r = np.maximum(short[:, :horizon].mean(0)[None, :]
                   + deck.spr[:, None], 0.0)        # E[short]+spread, floored
    acc = deck.bal[:, None] * r / 12.0
    inc = (acc * (deck.sign[:, None] > 0)).sum(0)
    exp = (acc * (deck.sign[:, None] < 0)).sum(0)
    return inc, exp


def mm_earning_assets(deck: MMDeck) -> float:
    """Asset-side balance for the NIM denominator (constant balances)."""
    return float(deck.bal[deck.sign > 0].sum())
