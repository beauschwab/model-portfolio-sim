"""Market conventions: day count fractions, holiday calendars, business-day
adjustment, coupon schedule generation, MBS payment delay.

The Monte Carlo engine runs on an idealized monthly grid (model time);
conventions enter through (a) exact accrual year fractions per period
(coupon AMOUNTS are convention-exact), (b) pay-date -> grid-month mapping,
(c) payment-delay discounting. Residual timing error from snapping pay
dates to month-ends is <= ~half a month of discounting; daily deflator
interpolation would remove it (documented, not built).
"""
from __future__ import annotations

import datetime as dt
from dataclasses import dataclass, field
from enum import Enum


class DayCount(Enum):
    THIRTY_360 = "30/360"        # US bond basis
    ACT_360 = "ACT/360"
    ACT_365F = "ACT/365F"
    ACT_ACT = "ACT/ACT"          # ISDA


class BDC(Enum):
    FOLLOWING = "F"
    MODIFIED_FOLLOWING = "MF"
    PRECEDING = "P"
    NONE = "NONE"


def year_fraction(d1: dt.date, d2: dt.date, basis: DayCount) -> float:
    if basis is DayCount.THIRTY_360:
        dd1 = min(d1.day, 30)
        dd2 = 30 if (d2.day == 31 and dd1 == 30) else d2.day
        return ((d2.year - d1.year) * 360 + (d2.month - d1.month) * 30
                + (dd2 - dd1)) / 360.0
    days = (d2 - d1).days
    if basis is DayCount.ACT_360:
        return days / 360.0
    if basis is DayCount.ACT_365F:
        return days / 365.0
    # ACT/ACT ISDA: split across year boundaries
    if d1.year == d2.year:
        denom = 366.0 if _is_leap(d1.year) else 365.0
        return days / denom
    yf = (dt.date(d1.year + 1, 1, 1) - d1).days \
        / (366.0 if _is_leap(d1.year) else 365.0)
    yf += (d2 - dt.date(d2.year, 1, 1)).days \
        / (366.0 if _is_leap(d2.year) else 365.0)
    yf += (d2.year - d1.year - 1)
    return yf


def _is_leap(y: int) -> bool:
    return y % 4 == 0 and (y % 100 != 0 or y % 400 == 0)


# --- holiday calendar -----------------------------------------------------------
def _nth_weekday(y, month, weekday, n) -> dt.date:
    d = dt.date(y, month, 1)
    off = (weekday - d.weekday()) % 7
    return d + dt.timedelta(days=off + 7 * (n - 1))


def _last_weekday(y, month, weekday) -> dt.date:
    d = dt.date(y, month + 1, 1) - dt.timedelta(days=1) if month < 12 \
        else dt.date(y, 12, 31)
    off = (d.weekday() - weekday) % 7
    return d - dt.timedelta(days=off)


def _observed(d: dt.date) -> dt.date:
    if d.weekday() == 5:
        return d - dt.timedelta(days=1)
    if d.weekday() == 6:
        return d + dt.timedelta(days=1)
    return d


def _easter(y: int) -> dt.date:
    """Anonymous Gregorian computus."""
    a = y % 19
    b, c = divmod(y, 100)
    d, e = divmod(b, 4)
    g = (8 * b + 13) // 25
    h = (19 * a + b - d - g + 15) % 30
    i, k = divmod(c, 4)
    l = (32 + 2 * e + 2 * i - h - k) % 7
    m = (a + 11 * h + 22 * l) // 451
    month, day = divmod(h + l - 7 * m + 114, 31)
    return dt.date(y, month, day + 1)


def us_bond_holidays(year: int) -> set[dt.date]:
    """SIFMA-style US bond market holidays, rule-generated (no early closes)."""
    return {
        _observed(dt.date(year, 1, 1)),
        _nth_weekday(year, 1, 0, 3),                       # MLK
        _nth_weekday(year, 2, 0, 3),                       # Presidents
        _easter(year) - dt.timedelta(days=2),              # Good Friday
        _last_weekday(year, 5, 0),                         # Memorial
        _observed(dt.date(year, 6, 19)),                   # Juneteenth
        _observed(dt.date(year, 7, 4)),
        _nth_weekday(year, 9, 0, 1),                       # Labor
        _nth_weekday(year, 10, 0, 2),                      # Columbus
        _observed(dt.date(year, 11, 11)),                  # Veterans
        _nth_weekday(year, 11, 3, 4),                      # Thanksgiving
        _observed(dt.date(year, 12, 25)),
    }


@dataclass
class Calendar:
    """Weekend + holiday calendar. Default holiday rule: US bond market."""
    name: str = "US"
    extra_holidays: set = field(default_factory=set)
    _cache: dict = field(default_factory=dict, repr=False)

    def is_business_day(self, d: dt.date) -> bool:
        if d.weekday() >= 5:
            return False
        y = d.year
        if y not in self._cache:
            self._cache[y] = us_bond_holidays(y) if self.name == "US" else set()
        return d not in self._cache[y] and d not in self.extra_holidays

    def adjust(self, d: dt.date, bdc: BDC) -> dt.date:
        if bdc is BDC.NONE:
            return d
        step = -1 if bdc is BDC.PRECEDING else 1
        out = d
        while not self.is_business_day(out):
            out += dt.timedelta(days=step)
        if bdc is BDC.MODIFIED_FOLLOWING and out.month != d.month:
            out = d
            while not self.is_business_day(out):
                out -= dt.timedelta(days=1)
        return out

    def add_business_days(self, d: dt.date, n: int) -> dt.date:
        out, step = d, 1 if n >= 0 else -1
        for _ in range(abs(n)):
            out += dt.timedelta(days=step)
            while not self.is_business_day(out):
                out += dt.timedelta(days=step)
        return out


def _add_months(d: dt.date, n: int) -> dt.date:
    y, m = divmod(d.year * 12 + d.month - 1 + n, 12)
    day = min(d.day, [31, 29 if _is_leap(y) else 28, 31, 30, 31, 30,
                      31, 31, 30, 31, 30, 31][m])
    return dt.date(y, m + 1, day)


def gen_schedule(effective: dt.date, maturity: dt.date, freq_months: int,
                 basis: DayCount, cal: Calendar,
                 bdc: BDC = BDC.MODIFIED_FOLLOWING):
    """Backward-rolled coupon schedule, short first stub.
    Returns list of (accr_start, accr_end, pay_date, tau). Accrual on
    adjusted dates (note: 30/360 markets often accrue unadjusted -- switch
    by adjusting only pay_date if needed)."""
    ends = []
    d = maturity
    while d > effective:
        ends.append(d)
        d = _add_months(maturity, -freq_months * len(ends))
    ends = list(reversed(ends))
    out = []
    prev = cal.adjust(effective, bdc)
    for e in ends:
        pay = cal.adjust(e, bdc)
        out.append((prev, pay, pay, year_fraction(prev, pay, basis)))
        prev = pay
    return out


def months_from(asof: dt.date, d: dt.date) -> int:
    """Pay date -> simulation month index m (cf at m discounts to (m+1)/12).
    Clamped at 0."""
    return max(0, (d.year - asof.year) * 12 + (d.month - asof.month) - 1)


# --- MBS payment delay -------------------------------------------------------------
# Stated delay days by program (settlement to first payment minus 30):
MBS_DELAY_DAYS = {"FN": 24, "FH": 14, "G1": 14, "G2": 19}


def delay_years(program_or_days) -> float:
    if isinstance(program_or_days, str):
        return MBS_DELAY_DAYS.get(program_or_days, 24) / 365.0
    return float(program_or_days) / 365.0
