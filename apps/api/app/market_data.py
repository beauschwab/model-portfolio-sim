"""Public research feeds. Network/normalization only; quant transforms live in engine.

Fetches never mutate the active market. Immutable, hash-checked snapshots retain
raw responses and normalized observations. Functions over module state preserve
the API repository seam. No user-provided URLs or credentials in saved records.
"""
from __future__ import annotations

import csv
import hashlib
import io
import json
import math
import os
from pathlib import Path
import re
import threading
from datetime import date, datetime, timedelta, timezone
from html.parser import HTMLParser
from urllib.parse import urljoin, urlparse
import xml.etree.ElementTree as ET

import httpx
from pydantic import BaseModel, ConfigDict, Field, FiniteFloat, model_validator

ROOT = Path(__file__).resolve().parents[3] / ".data" / "market-data"
_LOCK = threading.RLock()
MAX_BYTES = 40 * 1024 * 1024
MAX_ROWS = 100_000
VERSION = 1


class DataError(ValueError):
    pass


def entry(label, provider, access, unit, use, notes):
    return dict(label=label, provider=provider, access=access, unit=unit, use=use, notes=notes)


CATALOG = {
    "eris_sofr": entry("SOFR discount curve", "Eris", "public", "discount_factor", "curve",
        "Research use subject to Eris terms. Engine rates are derived from DFs; volatility and behavioral histories remain unchanged."),
    "eris_options": entry("SOFR option settlements", "Eris", "public", "mixed", "reference",
        "Futures options, not a complete OTC ATM swaption surface. Volatility conventions and coverage require calibration."),
    "nyfed": entry("Overnight reference rates", "New York Fed", "public", "decimal_rate", "reference",
        "SOFR/EFFR/OBFR/TGCR/BGCR fixings; SOFR averages are backward-looking, not Term SOFR."),
    "treasury": entry("Treasury nominal and real par curves", "US Treasury", "public", "decimal_rate", "reference",
        "Par Treasury yields, not SOFR discount factors. No automatic replacement of the swap curve."),
    "fed_zero": entry("Fitted Treasury zero curves", "Federal Reserve", "public", "decimal_rate", "reference",
        "GSW fitted continuously compounded nominal zero yields; source history can be revised."),
    "fred": entry("Macro and credit series", "FRED", "api_key", "source_units", "reference",
        "Set FRED_API_KEY server-side. Series-specific licenses apply. ICE OAS history is limited to three years from April 2026."),
    "pmms": entry("Primary mortgage rates", "Freddie Mac", "public", "decimal_rate", "reference",
        "Weekly primary borrower rates, not secondary MBS current coupons. Methodology changed in November 2022."),
    "fhfa": entry("House price histories", "FHFA", "public", "index", "reference",
        "Purchase-only traditional HPI; specify place_id (USA by default). Period dates are not publication dates."),
    "fdic": entry("Bank financial history", "FDIC", "public", "USD_thousands", "reference",
        "Requires FDIC certificate number. Bank legal entity, not necessarily the consolidated holding company."),
    "fdic_rates": entry("National deposit and CD rates", "FDIC", "public", "decimal_rate", "reference",
        "Monthly national benchmarks, not account betas, runoff assumptions, or bank funding curves."),
    "sec": entry("Company financial facts", "SEC EDGAR", "user_agent", "source_units", "reference",
        "Requires CIK and SEC_USER_AGENT with contact information. Standard taxonomy facts; excludes custom-tag disclosures."),
    "fed_stress": entry("Supervisory scenario paths", "Federal Reserve", "public", "source_units", "reference",
        "Baseline/adverse paths plus jump-off history. Hypothetical assumptions; conditional income replay is separate from instantaneous valuation shocks."),
    "fed_sep": entry("Policy and economic projections (SEP)", "Federal Reserve", "public", "percent", "forecast",
        "Published median and ranges. Rounded annual policy targets, not a joint forecast or a full curve. Range endpoints are not probabilities."),
    "philly_spf": entry("Professional forecaster consensus", "Philadelphia Fed", "public", "percent", "forecast",
        "Latest median workbook: five quarterly rate/unemployment forecasts. Cross-variable medians are a composite; current workbook revisions are not historical vintages."),
    "nyfed_sme": entry("Market expectations survey", "New York Fed", "public", "decimal_rate", "forecast",
        "Combined-panel policy-rate median and quartiles. Conservative results cutoff: first day of second month after survey. Questionnaire date is not results publication date; quartiles are disagreement, not joint scenarios."),
    "ffiec": entry("Call reports and UBPR", "FFIEC", "registered_download", "source_units", "reference",
        "Use the official CDR bulk/web-service download, then import normalized observations; no account enrollment or terms acceptance is automated."),
    "pooltalk": entry("MBS collateral disclosures", "Fannie Mae", "registered_download", "source_units", "reference",
        "Import authorized normalized extracts. No market prices. Internal research and redistribution have different terms."),
    "freddie_loans": entry("Mortgage performance histories", "Freddie Mac", "registered_download", "source_units", "reference",
        "Import authorized normalized extracts; large loan-level files require cohort aggregation before loading. No automatic prepay fit."),
}


class FetchRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    dataset: str
    as_of: date = Field(default_factory=date.today)
    start: date | None = None
    series: list[str] = Field(default_factory=list, max_length=12)
    identifier: str = Field("", max_length=40, pattern=r"^[A-Za-z0-9_-]*$")

    @model_validator(mode="after")
    def validate_request(self):
        if self.dataset not in CATALOG:
            raise ValueError("unknown research dataset")
        if self.as_of > date.today() or self.as_of.year < 1990:
            raise ValueError("as_of must be between 1990 and today")
        if self.start is None:
            self.start = self.as_of - timedelta(days=365 * 5)
        if self.start > self.as_of or (self.as_of - self.start).days > 365 * 40:
            raise ValueError("history window must be ordered and at most 40 years")
        if any(not re.fullmatch(r"[A-Za-z0-9_.-]{1,80}", s) for s in self.series):
            raise ValueError("invalid series identifier")
        if self.dataset in ("fdic", "sec") and not re.fullmatch(r"\d{1,10}", self.identifier):
            raise ValueError("provide a numeric FDIC certificate or SEC CIK")
        return self


class ApplyRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    snapshot_id: str = Field(pattern=r"^[a-f0-9]{64}$")
    expected_revision: int = Field(ge=0)


class ImportedObservation(BaseModel):
    model_config = ConfigDict(extra="forbid")
    date: date
    series: str = Field(min_length=1, max_length=160)
    value: FiniteFloat
    unit: str = Field(min_length=1, max_length=80)
    classification: str = Field(pattern=r"^(observed|derived|assumed)$")


class ImportRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    dataset: str
    as_of: date
    source_description: str = Field(min_length=1, max_length=1000)
    observations: list[ImportedObservation] = Field(min_length=1, max_length=MAX_ROWS)

    @model_validator(mode="after")
    def valid_import(self):
        if self.dataset not in ("ffiec", "pooltalk", "freddie_loans"):
            raise ValueError("file import is for FFIEC, PoolTalk and Freddie loan extracts")
        if self.as_of > date.today() or any(r.date > self.as_of for r in self.observations):
            raise ValueError("import dates cannot follow as_of or today")
        return self


def import_snapshot(req: ImportRequest):
    raw = _json(req.model_dump(mode="json"))
    rows = [r.model_dump(mode="json") for r in req.observations]
    payload = dict(schema_version=VERSION, dataset=req.dataset, as_of=str(req.as_of),
        fetched_at=datetime.now(timezone.utc).isoformat(), request={"mode": "authorized_extract"},
        sources=[dict(description=req.source_description, sha256=_hash(raw), bytes=len(raw))],
        observations=rows, observation_count=len(rows), curve=None,
        warnings=[CATALOG[req.dataset]["notes"], "User-supplied normalized extract; provenance and aggregation are not independently verified."])
    return save_snapshot(payload, [raw])


def catalog():
    return [{"id": k, **v, "configured":
             bool(os.getenv("FRED_API_KEY")) if k == "fred" else
             bool(os.getenv("SEC_USER_AGENT")) if k == "sec" else
             v["access"] != "registered_download"} for k, v in CATALOG.items()]


def _root():
    return Path(os.getenv("MARKET_DATA_DIR", str(ROOT)))


def _json(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()


def _hash(value):
    return hashlib.sha256(value).hexdigest()


def _atomic(path, payload):
    import uuid
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix("." + uuid.uuid4().hex + ".tmp")
    try:
        tmp.write_bytes(payload)
        tmp.replace(path)
    finally:
        tmp.unlink(missing_ok=True)


def save_snapshot(payload, raw_files):
    encoded = _json(payload)
    sid = _hash(encoded)
    from . import persistence
    if persistence.REPO is not None:
        raw_refs = [persistence.CODEC.objects.put(raw, 'bin') for raw in raw_files]
        manifest = persistence.CODEC.dump({'payload': payload, 'raw_files': raw_refs})
        persistence.REPO.save_research(sid, manifest)
        return {'id': sid, **payload}
    with _LOCK:
        for raw in raw_files:
            _atomic(_root() / "raw" / (_hash(raw) + ".bin"), raw)
        _atomic(_root() / "snapshots" / (sid + ".json"), encoded)
    return {"id": sid, **payload}


def get_snapshot(sid):
    if not re.fullmatch(r"[a-f0-9]{64}", sid):
        raise DataError("invalid snapshot identifier")
    from . import persistence
    if persistence.REPO is not None:
        payload = persistence.CODEC.load(persistence.REPO.research(sid)['manifest'])['payload']
        if _hash(_json(payload)) != sid or payload.get('schema_version') != VERSION:
            raise DataError('snapshot integrity or version check failed')
        return {'id': sid, **payload}
    try:
        raw = (_root() / "snapshots" / (sid + ".json")).read_bytes()
    except FileNotFoundError:
        raise KeyError(sid) from None
    if _hash(raw) != sid:
        raise DataError("snapshot integrity check failed")
    payload = json.loads(raw)
    if payload.get("schema_version") != VERSION:
        raise DataError("unsupported snapshot version")
    return {"id": sid, **payload}


def summary(s):
    from .forecast_data import metadata
    return {**{k: s[k] for k in ("id", "dataset", "as_of", "fetched_at", "observation_count", "warnings", "curve")},
            "forecast": metadata(s)}


def list_snapshots():
    from . import persistence
    if persistence.REPO is not None:
        return [summary(get_snapshot(row['id'])) for row in persistence.REPO.research()]
    files = sorted((_root() / "snapshots").glob("*.json"), key=lambda p: p.stat().st_mtime, reverse=True)[:100]
    return [summary(get_snapshot(p.stem)) for p in files]


class FeedClient:
    def __init__(self, transport=None):
        self.client = httpx.Client(timeout=httpx.Timeout(30, connect=10), follow_redirects=False,
                                   transport=transport, headers={"User-Agent": "RatesWorkbench/0.1 research"})
        self.raw_files = []
        self.sources = []

    def close(self):
        self.client.close()

    def get(self, url, params=None, headers=None, binary=False):
        # URLs are selected exclusively by adapters, never accepted from callers.
        try:
            with self.client.stream("GET", url, params=params, headers=headers) as response:
                if response.status_code != 200:
                    raise DataError(f"{urlparse(url).hostname}: upstream HTTP {response.status_code}")
                chunks, total = [], 0
                for chunk in response.iter_bytes():
                    total += len(chunk)
                    if total > MAX_BYTES:
                        raise DataError("upstream response exceeds 40 MiB")
                    chunks.append(chunk)
                raw = b"".join(chunks)
        except httpx.HTTPError:
            # Do not expose exception URLs: FRED credentials travel in query params.
            raise DataError(f"{urlparse(url).hostname}: network request failed; retry later") from None
        self.raw_files.append(raw)
        safe_params = {k: v for k, v in (params or {}).items() if k != "api_key"}
        self.sources.append(dict(url=url, parameters=safe_params, sha256=_hash(raw), bytes=len(raw)))
        return raw if binary else raw.decode("utf-8-sig")


def csv_rows(text, header=None):
    lines = text.splitlines()
    if header:
        idx = next((i for i, line in enumerate(lines) if line.startswith(header)), None)
        if idx is None:
            raise DataError(f"missing CSV header {header}")
        lines = lines[idx:]
    return csv.DictReader(io.StringIO("\n".join(lines)))


def parse_date(s):
    s = str(s).strip()
    for fmt in ("%Y-%m-%d", "%m/%d/%Y", "%Y%m%d", "%d-%b-%Y"):
        try:
            return datetime.strptime(s[:10] if fmt == "%Y-%m-%d" else s, fmt).date()
        except ValueError:
            pass
    raise DataError(f"invalid observation date: {s[:30]}")


def number(value):
    if value is None or str(value).strip() in ("", ".", "NA", "N/A", "ND", "null"):
        return None
    try:
        v = float(str(value).replace(",", "").strip().rstrip("%"))
    except ValueError:
        raise DataError("unexpected nonnumeric source value") from None
    if not math.isfinite(v):
        raise DataError("non-finite source value")
    return v


def obs(day, series, value, unit, classification="observed", **attrs):
    return dict(date=str(day), series=series, value=value, unit=unit,
                classification=classification, **attrs)


def eris(req, feed):
    root = "https://files.erisfutures.com/ftp/"
    listing = feed.get(root)
    pattern = (r'href="(Eris_(\d{8})_EOD_DiscountFactors_SOFR\.csv)"' if req.dataset == "eris_sofr"
               else r'href="(Eris_Options_(\d{8})_Settles\.csv)"')
    candidates = [(f, parse_date(d)) for f, d in re.findall(pattern, listing)]
    # Recent root plus the requested archive month; no silent later-date substitution.
    valid = [(f, d, root) for f, d in candidates if d <= req.as_of and (req.as_of - d).days <= 7]
    if not valid:
        for day in (req.as_of, req.as_of - timedelta(days=7)):
            archive = root + f"archives/{day.year}/{day.strftime('%m-%B')}/"
            try:
                listing = feed.get(archive)
            except DataError:
                continue
            valid.extend((f, parse_date(d), archive) for f, d in re.findall(pattern, listing)
                         if parse_date(d) <= req.as_of and (req.as_of - parse_date(d)).days <= 7)
    if not valid:
        raise DataError("no Eris file within seven days before the requested date")
    filename, day, base = max(valid, key=lambda x: x[1])
    rows = list(csv_rows(feed.get(base + filename)))
    if req.dataset == "eris_sofr":
        from portfolio_risk.core.curve import market_discount_factors_to_par
        points = [(parse_date(r["Date"]), number(r["DiscountFactor"])) for r in rows]
        curve = market_discount_factors_to_par([(d - day).days / 365 for d, _ in points], [v for _, v in points])
        curve["as_of"] = day.isoformat()
        return [obs(day, "SOFR_DF", v, "discount_factor", "derived", maturity=d.isoformat()) for d, v in points], curve
    result = []
    for r in rows:
        if parse_date(r["EvaluationDate"]) != day:
            raise DataError("option file valuation date mismatch")
        for field, unit in (("Price", "price_points"), ("Rate Vol Nor", "percent_rate_per_sqrt_year"),
                            ("Volatility", "decimal_futures_price_vol")):
            v = number(r.get(field))
            if v is not None:
                result.append(obs(day, r["Symbol"] + ":" + field, v, unit, "derived",
                                  expiry=r["ExpiryDate"], underlying=r["UnderlyingSymbol"],
                                  underlying_tenor=r["UnderlyingTenor"], strike=r["StrikePrice"]))
    return result, None


def nyfed(req, feed):
    paths = {"SOFR": "secured/sofr", "EFFR": "unsecured/effr", "OBFR": "unsecured/obfr",
             "TGCR": "secured/tgcr", "BGCR": "secured/bgcr", "SOFR-AVG": "secured/sofrai"}
    result = []
    for series in req.series or ["SOFR", "EFFR", "OBFR", "TGCR", "BGCR", "SOFR-AVG"]:
        if series not in paths:
            raise DataError("unsupported NY Fed series")
        payload = json.loads(feed.get(f"https://markets.newyorkfed.org/api/rates/{paths[series]}/search.json",
            params={"startDate": str(req.start), "endDate": str(req.as_of)}))
        for row in payload["refRates"]:
            day = parse_date(row["effectiveDate"])
            fields = [("percentRate", series)] if series != "SOFR-AVG" else [
                ("average30day", "SOFR_AVG30"), ("average90day", "SOFR_AVG90"),
                ("average180day", "SOFR_AVG180"), ("index", "SOFR_INDEX")]
            for field, name in fields:
                v = number(row.get(field))
                if v is not None:
                    result.append(obs(day, name, v if field == "index" else v / 100,
                                      "index" if field == "index" else "decimal_rate"))
    return result, None


def treasury(req, feed):
    if req.as_of.year - req.start.year > 10:
        raise DataError("Treasury requests are limited to eleven calendar years per snapshot")
    result = []
    for year in range(req.start.year, req.as_of.year + 1):
        for kind, prefix in (("daily_treasury_yield_curve", "UST"), ("daily_treasury_real_yield_curve", "TIPS")):
            doc = feed.get("https://home.treasury.gov/resource-center/data-chart-center/interest-rates/pages/xml",
                           params={"data": kind, "field_tdr_date_value": str(year)})
            if "<!DOCTYPE" in doc or "<!ENTITY" in doc:
                raise DataError("unexpected XML document type")
            for element in ET.fromstring(doc).iter():
                if element.tag.split("}")[-1] != "properties":
                    continue
                row = {x.tag.split("}")[-1]: x.text for x in element}
                day = parse_date(row["NEW_DATE"])
                for key, value in row.items():
                    if re.fullmatch(r"(?:BC|TC)_[A-Z0-9_]+", key):
                        v = number(value)
                        if v is not None:
                            result.append(obs(day, prefix + ":" + key, v / 100, "decimal_rate", "derived"))
    return result, None


def fed_zero(req, feed):
    rows = csv_rows(feed.get("https://www.federalreserve.gov/data/yield-curve-tables/feds200628.csv"), "Date,")
    return [obs(parse_date(r["Date"]), k, v / 100, "decimal_rate", "derived")
            for r in rows for k in r if k.startswith("SVENY") and (v := number(r[k])) is not None], None


def fred(req, feed):
    key = os.getenv("FRED_API_KEY")
    if not key:
        raise DataError("set FRED_API_KEY on the API server")
    result = []
    for series in req.series or ["DPRIME", "UNRATE", "CPIAUCSL", "GDPC1", "BAMLC0A0CM"]:
        params = {"api_key": key, "file_type": "json", "series_id": series}
        info = json.loads(feed.get("https://api.stlouisfed.org/fred/series", params=params))["seriess"][0]
        payload = json.loads(feed.get("https://api.stlouisfed.org/fred/series/observations", params={
            **params, "observation_start": str(req.start), "observation_end": str(req.as_of),
            "realtime_start": str(req.as_of), "realtime_end": str(req.as_of), "limit": 100000}))
        feed.sources[-1]["series_metadata"] = {k: info.get(k) for k in ("id", "title", "units", "frequency", "notes")}
        if int(payload.get("count", 0)) > 100000:
            raise DataError("FRED response requires a narrower history window")
        for r in payload["observations"]:
            v = number(r["value"])
            if v is not None:
                result.append(obs(parse_date(r["date"]), series, v, info["units"], vintage=str(req.as_of)))
    return result, None


def pmms(req, feed):
    rows = csv_rows(feed.get("https://www.freddiemac.com/pmms/docs/PMMS_history.csv"))
    return [obs(parse_date(r["date"]), k, v / 100, "decimal_rate") for r in rows
            for k in ("pmms30", "pmms15") if (v := number(r.get(k))) is not None], None


def fhfa(req, feed):
    rows = csv_rows(feed.get("https://www.fhfa.gov/hpi/download/monthly/hpi_master.csv"))
    result = []
    for r in rows:
        if (r["place_id"] != (req.identifier or "USA") or r["hpi_type"] != "traditional"
                or r["hpi_flavor"] != "purchase-only" or r["frequency"] not in ("monthly", "quarterly")):
            continue
        month = int(r["period"]) if r["frequency"] == "monthly" else 3 * int(r["period"]) - 2
        day = date(int(r["yr"]), month, 1)
        for field in ("index_sa", "index_nsa"):
            v = number(r.get(field))
            if v is not None:
                result.append(obs(day, f"{r['place_id']}:{r['frequency']}:{field}", v, "index", "derived"))
    return result, None


def fdic(req, feed):
    fields = ["ASSET", "DEP", "EQ", "NETINC", "INTINC", "EINTEXP", "NIMY"]
    payload = json.loads(feed.get("https://api.fdic.gov/banks/financials", params={
        "filters": f"CERT:{int(req.identifier)} AND REPDTE:[{req.start:%Y%m%d} TO {req.as_of:%Y%m%d}]",
        "fields": ",".join(["CERT", "NAME", "REPDTE", *fields]), "limit": 10000, "format": "json"}))
    if int(payload.get("meta", {}).get("total", 0)) > 10000:
        raise DataError("FDIC result truncated")
    result = []
    for item in payload["data"]:
        row = item["data"]
        for field in fields:
            if (v := number(row.get(field))) is not None:
                result.append(obs(parse_date(row["REPDTE"]), field, v,
                                  "percent" if field == "NIMY" else "USD_thousands", institution=row.get("NAME")))
    return result, None


def sec(req, feed):
    ua = os.getenv("SEC_USER_AGENT")
    if not ua:
        raise DataError("set SEC_USER_AGENT with organization and contact email on the API server")
    payload = json.loads(feed.get(f"https://data.sec.gov/api/xbrl/companyfacts/CIK{int(req.identifier):010d}.json",
                                  headers={"User-Agent": ua}))
    result = []
    fields = req.series or ["Assets", "Liabilities", "StockholdersEquity", "NetIncomeLoss"]
    for field in fields:
        fact = payload.get("facts", {}).get("us-gaap", {}).get(field, {})
        for unit, rows in fact.get("units", {}).items():
            for r in rows:
                if parse_date(r["filed"]) > req.as_of:
                    continue
                v = number(r.get("val"))
                if v is not None:
                    result.append(obs(parse_date(r["end"]), field, v, unit, filed=r["filed"],
                        start=r.get("start"), accession=r["accn"], form=r["form"], frame=r.get("frame")))
    return result, None


class Tables(HTMLParser):
    def __init__(self):
        super().__init__()
        self.rows, self.row, self.cell = [], [], None

    def handle_starttag(self, tag, attrs):
        if tag == "tr":
            self.row = []
        elif tag in ("td", "th"):
            self.cell = ""

    def handle_data(self, data):
        if self.cell is not None:
            self.cell += data

    def handle_endtag(self, tag):
        if tag in ("td", "th") and self.cell is not None:
            self.row.append(" ".join(self.cell.split()))
            self.cell = None
        elif tag == "tr" and self.row:
            self.rows.append(self.row)


def fdic_rates(req, feed):
    # Archive pages retain the month in the URL; always validate a table header.
    slug = req.as_of.strftime("%B-%Y").lower()
    url = f"https://www.fdic.gov/national-rates-and-rate-caps/national-rates-and-rate-caps-{slug}"
    page = feed.get(url)
    parser = Tables()
    parser.feed(page)
    is_rate = lambda c: bool(re.search(r"National (?:Deposit )?Rates?", c)) and "Cap" not in c
    header = next((r for r in parser.rows if any(is_rate(c) for c in r)), None)
    if not header:
        raise DataError("FDIC deposit table layout changed")
    idx = next(i for i, c in enumerate(header) if is_rate(c))
    # Ignore sidebar news dates: the rate's effective date is the table subheading.
    dates = re.findall(r"Revised Rule\s+((?:January|February|March|April|May|June|July|August|September|October|November|December)\s+\d{1,2},\s+\d{4})", page)
    days = [datetime.strptime(d, "%B %d, %Y").date() for d in dates]
    day = next((d for d in days if d.year == req.as_of.year and d.month == req.as_of.month), None)
    if day is None or day > req.as_of:
        raise DataError("requested month's FDIC table was not yet published at as_of")
    result = []
    for r in parser.rows:
        if len(r) > idx and re.search(r"Savings|Checking|Money Market|month|year", r[0], re.I):
            match = re.fullmatch(r"\s*(\d+(?:\.\d+)?)%?\s*", r[idx])
            if match:
                result.append(obs(day, r[0], float(match[1]) / 100, "decimal_rate"))
    return result, None


def fed_stress(req, feed):
    year = req.as_of.year
    page_url = f"https://www.federalreserve.gov/supervisionreg/dfa-stress-tests-{year}.htm"
    page = feed.get(page_url)
    release_dates = re.findall(r"final-supervisory-stress-test-scenarios-(\d{8})\.pdf", page, re.I)
    if release_dates and min(datetime.strptime(d, "%Y%m%d").date() for d in release_dates) > req.as_of:
        raise DataError("final supervisory package was published after as_of")
    links = re.findall(r'href=[\"\']([^\"\']+\.csv)[\"\']', page, re.I)
    links = sorted({urljoin(page_url, s) for s in links if "domestic" in s.lower()
                    and "propos" not in s.lower()})
    if not links:
        raise DataError("no final domestic scenario CSVs found for the selected year")
    if len(links) > 6 or any(urlparse(s).hostname != "www.federalreserve.gov" for s in links):
        raise DataError("unexpected scenario download links")
    result = []
    for url in links:
        scenario = "history" if "histor" in url.lower() else "adverse" if "adverse" in url.lower() else "baseline"
        for r in csv_rows(feed.get(url)):
            period = r.get("Date", "").strip()
            match = re.fullmatch(r"(\d{4})[: -]?Q([1-4])", period, re.I)
            if not match:
                raise DataError("unexpected supervisory scenario period")
            day = date(int(match[1]), 3 * int(match[2]) - 2, 1)
            for field, value in r.items():
                if field in ("Date", "Scenario Name"):
                    continue
                if (v := number(value)) is not None:
                    result.append(obs(day, scenario + ":" + field.strip(), v,
                                      "index" if "(Level)" in field else "percent",
                                      "observed" if scenario == "history" else "assumed", period=period))
    result.sort(key=lambda r: (r["series"].startswith("history:"), r["date"]))
    return result, None


ADAPTERS = {"eris_sofr": eris, "eris_options": eris, "nyfed": nyfed, "treasury": treasury,
            "fed_zero": fed_zero, "fred": fred, "pmms": pmms, "fhfa": fhfa, "fdic": fdic,
            "sec": sec, "fdic_rates": fdic_rates, "fed_stress": fed_stress}


def fetch_snapshot(req: FetchRequest, transport=None):
    from .forecast_data import DATASETS, fed_sep, philly_spf, nyfed_sme
    adapters = {**ADAPTERS, "fed_sep": fed_sep, "philly_spf": philly_spf, "nyfed_sme": nyfed_sme}
    if req.dataset not in adapters:
        raise DataError("this dataset requires an authorized file extract; use the import endpoint")
    feed = FeedClient(transport)
    try:
        rows, curve = adapters[req.dataset](req, feed)
        if req.dataset != "eris_sofr" and req.dataset not in DATASETS:
            rows = [r for r in rows if req.start <= parse_date(r["date"]) <= req.as_of]
        if not rows or len(rows) > MAX_ROWS:
            raise DataError("empty dataset or over 100,000 observations; check identifiers or narrow the window")
        warnings = [CATALOG[req.dataset]["notes"]]
        if req.dataset not in ("fred", "sec", "eris_sofr", "eris_options") and req.dataset not in DATASETS:
            warnings.append("Latest available source vintage, filtered by observation period. Not a point-in-time archive of what was known historically.")
        if req.dataset == "fed_stress":
            warnings.append("Scenario year selects the published annual package; as_of does not establish historical publication availability. Future scenario periods are intentionally retained.")
        if req.dataset in DATASETS:
            warnings.append("Published forecast/stress values are assumptions, not observed outcomes. Raw source bytes are retained; later corrections are possible. Source periods are preserved, including future periods.")
        last = max(r["date"] for r in rows)
        if req.dataset not in DATASETS and (req.as_of - parse_date(last)).days > 7:
            warnings.append(f"Latest observation is {last}; lower-frequency data or publication lag may explain the gap.")
        if curve:
            warnings.append("Ten-pillar projection is approximate; beyond 30y the engine retains its flat-zero extrapolation. Volatility, book dates/prices and behavioral histories are not refreshed.")
            if curve["max_zero_error_bp_30y"] > 5:
                warnings.append(f"Material curve approximation: maximum zero-rate deviation is {curve['max_zero_error_bp_30y']:.2f} bp. The engine has no curve pillars below one year.")
        payload = dict(schema_version=VERSION, dataset=req.dataset, request=req.model_dump(mode="json"),
                       as_of=str(req.as_of), fetched_at=datetime.now(timezone.utc).isoformat(),
                       sources=feed.sources, observations=rows, observation_count=len(rows),
                       warnings=warnings, curve=curve)
        return save_snapshot(payload, feed.raw_files)
    except DataError:
        raise
    except (ValueError, KeyError, TypeError, IndexError, ET.ParseError) as exc:
        raise DataError(f"{req.dataset}: source schema changed or response is invalid ({type(exc).__name__})") from None
    finally:
        feed.close()
