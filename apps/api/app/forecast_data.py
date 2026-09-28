"""Public forecast parsing. Source units and period conventions are preserved.

No curve interpolation or valuation belongs here; the engine owns that work.
"""
from datetime import date, datetime
import io
import re
from urllib.parse import urljoin, urlparse
from zipfile import ZipFile, BadZipFile

import openpyxl

from .market_data import DataError, Tables, number, obs

DATASETS = {"fed_stress", "fed_sep", "philly_spf", "nyfed_sme"}
FED_FIELDS = {
    "3-month Treasury rate": "short_rate", "5-year Treasury yield": "rate_5y",
    "10-year Treasury yield": "rate_10y", "Mortgage rate": "mortgage_rate",
    "House Price Index (Level)": "hpi", "BBB corporate yield": "bbb_yield",
    "Unemployment rate": "unemployment", "Real GDP growth": "gdp_growth",
    "CPI inflation rate": "cpi", "Prime rate": "prime",
}


def workbook(raw):
    # Bound decompression before handing a remote XLSX to the parser.
    try:
        with ZipFile(io.BytesIO(raw)) as z:
            if sum(f.file_size for f in z.infolist()) > 100 * 1024**2:
                raise DataError("forecast workbook exceeds decompressed size limit")
        return openpyxl.load_workbook(io.BytesIO(raw), data_only=True, read_only=True)
    except (BadZipFile, KeyError, ValueError):
        raise DataError("invalid forecast workbook") from None


def links(page, base, suffix):
    urls = {urljoin(base, s.replace("&amp;", "&")) for s in
            re.findall(r'href=["\']([^"\']+)["\']', page, re.I)
            if suffix in s.lower()}
    return sorted(u for u in urls if urlparse(u).scheme == "https"
                  and urlparse(u).hostname == urlparse(base).hostname)


def forecast_obs(day, variable, value, unit, scenario, convention, **attrs):
    return obs(day, f"{scenario}:{variable}", value, unit, "assumed",
               variable=variable, scenario=scenario, convention=convention, **attrs)


def fed_sep(req, feed):
    root = "https://www.federalreserve.gov/monetarypolicy/fomccalendars.htm"
    candidates = []
    for url in links(feed.get(root), root, "fomcprojtabl"):
        m = re.search(r"fomcprojtabl(\d{8})\.htm$", url)
        if m and (day := datetime.strptime(m[1], "%Y%m%d").date()) <= req.as_of:
            candidates.append((day, url))
    if not candidates:
        raise DataError("no published SEP release on or before as_of in the Fed calendar")
    released, url = max(candidates)
    raw = feed.get(url, binary=True)
    try:
        page = raw.decode("utf-8-sig")
    except UnicodeDecodeError:
        page = raw.decode("windows-1252")
    t = Tables(); t.feed(page)
    header = next((r for r in t.rows if r and r[0] == str(released.year)), None)
    if not header or "Longer run" not in header:
        raise DataError("SEP year header changed")
    periods = header[:header.index("Longer run") + 1]
    fields = {"Change in real GDP": ("gdp_growth", "q4_q4"),
              "Unemployment rate": ("unemployment", "q4_average"),
              "PCE inflation": ("pce", "q4_q4"),
              "Core PCE inflation": ("core_pce", "q4_q4"),
              "Federal funds rate": ("policy_rate", "year_end")}
    rows = []
    for row in t.rows:
        label = re.sub(r"\d+$", "", row[0])
        if label not in fields:
            continue
        variable, convention = fields[label]
        for period, value in zip(periods, row[1:1 + len(periods)]):
            v = number(value)
            if v is not None:
                rows.append(forecast_obs(released if period == "Longer run" else date(int(period), 12, 31),
                    variable, v, "percent", "median", "longer_run" if period == "Longer run" else convention,
                    release_date=str(released), period=period, statistic="median"))
        # Preserve published uncertainty ranges without treating them as probabilities.
        for group, offset in (("central_tendency", len(periods)), ("range", 2 * len(periods))):
            for period, value in zip(periods, row[1 + offset:1 + offset + len(periods)]):
                cleaned = value.strip().replace("−", "-")
                match = re.fullmatch(r"(-?\d+(?:\.\d+)?)\s*[–-]\s*(-?\d+(?:\.\d+)?)", cleaned)
                nums = list(match.groups()) if match else ([cleaned] if re.fullmatch(r"-?\d+(?:\.\d+)?", cleaned) else [])
                if nums and period != "Longer run":
                    for side, v in (("low", nums[0]), ("high", nums[-1])):
                        rows.append(forecast_obs(date(int(period), 12, 31), variable, float(v), "percent",
                            group + "_" + side, convention, release_date=str(released), period=period,
                            statistic=group + "_" + side))
        if label == "Federal funds rate":
            break
    if len([r for r in rows if r["scenario"] == "median"]) < 12:
        raise DataError("SEP median table incomplete")
    return rows, None


def philly_spf(req, feed):
    root = "https://www.philadelphiafed.org/surveys-and-data/real-time-data-research/median-forecasts"
    page = feed.get(root)
    dates = re.findall(r"last update:\s*([A-Za-z]+ \d{1,2}, \d{4})", page)
    if not dates:
        raise DataError("SPF workbook update date unavailable")
    released = max(datetime.strptime(s, "%B %d, %Y").date() for s in dates)
    if released > req.as_of:
        raise DataError("current SPF workbook was updated after as_of; historical vintage not available")
    urls = links(page, root, "medianlevel.xlsx")
    if len(urls) != 1:
        raise DataError("SPF median workbook link changed")
    w = workbook(feed.get(urls[0], binary=True))
    rows = []
    fields = {"TBILL": "short_rate", "TBOND": "rate_10y", "BOND": "aaa_yield",
              "BAABOND": "baa_yield", "UNEMP": "unemployment"}
    try:
        for sheet, variable in fields.items():
            data = list(w[sheet].values)
            eligible = [r for r in data[1:] if isinstance(r[0], (int, float))
                        and (int(r[0]), int(r[1])) <= (released.year, (released.month - 1) // 3 + 1)]
            r = max(eligible, key=lambda r: (r[0], r[1]))
            year, quarter = int(r[0]), int(r[1])
            for lead in range(5):
                # Column 1 is prior quarter actual; 2 is current-quarter forecast.
                key = f"{sheet}{lead + 2}"
                v = r[data[0].index(key)]
                if v in (None, "#N/A", "N.A."):
                    continue
                qi = year * 4 + quarter - 1 + lead
                day = date(qi // 4, (qi % 4) * 3 + 1, 1)
                rows.append(forecast_obs(day, variable, number(v), "percent", "median", "quarter_average",
                    release_date=str(released), vintage=f"{year}Q{quarter}", period=f"{day.year} Q{qi % 4 + 1}"))
    finally:
        w.close()
    if len({r["vintage"] for r in rows}) != 1:
        raise DataError("SPF sheets have inconsistent survey vintages")
    return rows, None


def nyfed_sme(req, feed):
    root = "https://www.newyorkfed.org/markets/market-intelligence/survey-of-market-expectations"
    page = feed.get(root)
    candidates = []
    for url in links(page, root, "-data.xlsx"):
        m = re.search(r"/(\d{4})/([a-z]{3})-\1-data.xlsx$", url)
        if not m or int(m[1]) < 2025:
            continue
        month = datetime.strptime(m[2], "%b").month
        # The workbook's survey_release_date is QUESTIONNAIRE release, not results.
        # Conservative embargo: first of second month after survey month. A dated
        # results URL alone does not prove historical publication availability.
        n = int(m[1]) * 12 + month - 1 + 2
        available_after = date(n // 12, n % 12 + 1, 1)
        if available_after <= req.as_of:
            candidates.append((int(m[1]), month, url, available_after))
    if not candidates:
        raise DataError("no SME results past the conservative publication cutoff")
    year, month, url, available_after = max(candidates)
    w = workbook(feed.get(url, binary=True))
    rows = []
    try:
        it = iter(w.active.values); header = next(it)
        required = {"panel_type", "question_tag", "horizon_date", "aggregation", "aggregation_value", "survey_release_date"}
        if not required <= set(header):
            raise DataError("SME workbook schema changed")
        for row in it:
            d = dict(zip(header, row))
            if d["panel_type"] != "Combined" or d["question_tag"] != "fftr_pathofmodes":
                continue
            if d["aggregation"] not in ("pctl25", "pctl50", "pctl75") or not d["horizon_date"]:
                continue
            day = date.fromisoformat(str(d["horizon_date"])[:10])
            value = number(d["aggregation_value"])
            if value is None:
                continue
            rows.append(forecast_obs(day, "policy_rate", value, "decimal_rate",
                {"pctl25": "p25", "pctl50": "median", "pctl75": "p75"}[d["aggregation"]], "date_end",
                period=str(d["horizon"]), vintage=f"{year}-{month:02}",
                questionnaire_date=str(d["survey_release_date"]), available_after=str(available_after)))
    finally:
        w.close()
    return rows, None


def normalized_rows(snapshot):
    """Also understands existing version-1 Fed snapshots without new metadata."""
    if snapshot["dataset"] != "fed_stress":
        return snapshot["observations"]
    result = []
    for row in snapshot["observations"]:
        scenario, _, field = row["series"].partition(":")
        result.append({**row, "scenario": scenario, "variable": FED_FIELDS.get(field, field),
                       "convention": "quarter_end" if field == "House Price Index (Level)" else "quarter_average"})
    return result


def metadata(snapshot):
    if snapshot["dataset"] not in DATASETS:
        return None
    rows = normalized_rows(snapshot)
    scenarios = sorted({r["scenario"] for r in rows if r["scenario"] != "history"})
    runnable = [s for s in scenarios if s in ("baseline", "adverse", "median")]
    return {"scenarios": scenarios, "runnable": runnable,
            "periods": sorted({r["date"][:7] + "-01" for r in rows
                               if r.get("variable") in ("short_rate", "policy_rate")
                               and r.get("convention") != "longer_run" and r["scenario"] in runnable}),
            "variables": sorted({r["variable"] for r in rows}),
            "alignment": "relative_replay"}
