"""Typed JSON round trips for product frames, validated before publication."""
from __future__ import annotations

import datetime as dt
import json
import math

import polars as pl


def normalize_book(name, rows, original, asof):
    if not rows:
        return original.clear()
    parsed = []
    for row in rows:
        row = dict(row)
        # JSON edits can contain numeric strings. Normalize against the existing
        # product schema before comparisons; NaN strings otherwise evade checks.
        for key, dtype in original.schema.items():
            if key in row and dtype.is_numeric():
                value = row[key]
                if value is None:
                    if original[key].null_count() == 0:
                        raise ValueError(f"{key} must not be null")
                    continue
                number = float(value)
                if not math.isfinite(number):
                    raise ValueError(f"{key} must be finite")
                if dtype.is_integer() and not number.is_integer():
                    raise ValueError(f"{key} must be an integer")
                row[key] = int(number) if dtype.is_integer() else number
        for key, value in row.items():
            if isinstance(value, (int, float)) and not math.isfinite(value):
                raise ValueError(f"{key} must be finite")
        if "maturity" in row:
            row["maturity"] = dt.date.fromisoformat(str(row["maturity"]))
            if row["maturity"] <= asof:
                raise ValueError("maturity must be after the valuation date")
        for key in ("call_schedule", "put_schedule", "sink_schedule"):
            if key in row:
                value = row[key]
                value = json.loads(value) if isinstance(value, str) else value
                row[key] = [(dt.date.fromisoformat(str(d)), float(v)) for d, v in (value or [])]
                if any(not math.isfinite(v) or v < 0 for _, v in row[key]):
                    raise ValueError(f"invalid {key} amount")
        for key in ("balance", "current_face", "face", "price"):
            if key in row and (row[key] is None or float(row[key]) < 0 or (key == "price" and row[key] == 0)):
                raise ValueError(f"{key} must be nonnegative (price must be positive)")
        if "freq_months" in row and row["freq_months"] not in (0, 1, 3, 6, 12):
            raise ValueError("unsupported payment frequency")
        if "is_float" in row and row["is_float"] not in (0, 1):
            raise ValueError("is_float must be 0 or 1")
        if name in ("loans", "debt") and row.get("freq_months") == 0:
            raise ValueError("corporate payment frequency must be positive")
        parsed.append(row)
    # Object columns contain heterogeneous (date, amount) tuples; do not let
    # inference stringify dates or coerce nested values to a common scalar type.
    schedules = {k for r in parsed for k in r if k.endswith("_schedule")}
    frame = pl.DataFrame([{k: v for k, v in r.items() if k not in schedules} for r in parsed], infer_schema_length=None)
    for key in schedules:
        frame = frame.with_columns(pl.Series(key, [r.get(key) for r in parsed], dtype=pl.Object))
    if name == "mbs":
        from portfolio_risk.core.scenarios import extract_sec
        extract_sec(frame)
        if any(float(r.get("factor", 0)) <= 0 or float(r.get("wam", 0)) <= 0 for r in parsed):
            raise ValueError("MBS factor and remaining term must be positive")
    elif name in ("loans", "debt"):
        from portfolio_risk.products.corp import CorpDeck
        CorpDeck(frame, asof)
    elif name == "cds":
        from portfolio_risk.products.cds import CDDeck
        CDDeck(frame, asof)
    elif name == "deposits":
        from portfolio_risk.products.deposits import DepositDeck, SEGMENTS
        if any(r.get("segment") not in SEGMENTS for r in parsed):
            raise ValueError("unknown deposit segment")
        DepositDeck(frame)
    elif name == "mm":
        from portfolio_risk.products.mm import MMDeck
        if any(r.get("side") not in ("asset", "liability") for r in parsed):
            raise ValueError("money-market side must be asset or liability")
        MMDeck(frame)
    ids = frame["cusip" if name == "mbs" else "id"]
    if ids.null_count() or ids.n_unique() != len(frame):
        raise ValueError("position identifiers must be present and unique")
    return frame
