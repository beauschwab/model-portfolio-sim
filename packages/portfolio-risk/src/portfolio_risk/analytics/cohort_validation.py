"""Independent cohort approximation audit; all product calculations use Rust.

Loan and representative calibrations are separate at the supplied base prices.
Scenario marks retain each instrument's own base OAS and common random numbers.
This is numerical approximation validation, not empirical model validation.
"""
from collections import defaultdict
import json
import math
import time

import polars as pl

from .cohorts import position_books
from .incremental import price_books
from .accounting import run_balance_sheet_nii
from ..core.dependency import DependencyCache
from ..core.runtime import run_context

RISK = ('market_value', 'dv01', 'nii')
FLOWS = ('principal', 'cash_interest', 'accrual_interest', 'book_amortization')


def _compare(cohort, product, scenario, month, metric, representative, individual, tolerance):
    absolute = tolerance.get('absolute', .01)
    relative = tolerance.get('relative', .01)
    if not all(math.isfinite(x) and x >= 0 for x in (absolute, relative)):
        raise ValueError('audit tolerances must be finite and nonnegative')
    if not all(math.isfinite(x) for x in (representative, individual)):
        raise ValueError('nonfinite cohort audit result')
    difference = representative - individual
    allowed = absolute + relative * abs(individual)
    return dict(cohort_id=cohort, product=product, shock_bp=scenario, month=month,
        metric=metric, representative=representative, individual=individual,
        difference=difference, absolute_error=abs(difference), allowed_error=allowed,
        relative_error=abs(difference/individual) if individual else None,
        passed=abs(difference) <= allowed)


def audit(build, *, products, asof, swap_rates, vol_pts, config, mbs_hists,
          dep_hist, seed=7, shocks=(0., -200., 200.), tolerances=None,
          batch_size=256, timeout=1800., cancelled=None, emit=None, progress=None):
    """Audit every selected original record, with bounded pricing/output batches.

emit receives immutable-sized Polars chunks: errors, suggestions, loan_risk,
loan_cashflows. Without a sink, only the compact summary is returned. Monthly
cashflow errors are base-only; stressed PV/DV01/NII use the fixed-OAS graph.
Tolerances are indexed by product then metric, with dollar absolute and decimal
relative allowances. Suggestions are proposed splits, never automatic changes.
"""
    if config.compute_backend != 'rust': raise ValueError('cohort audit requires Rust pricing')
    if type(batch_size) is not int or not 1 <= batch_size <= 256:
        raise ValueError('audit batch_size must be in [1,256]')
    if not math.isfinite(timeout) or not 0 < timeout <= 3600:
        raise ValueError('audit timeout must be in (0,3600] seconds')
    shocks = [float(s) for s in shocks]
    if not 1 <= len(shocks) <= 9 or len(set(shocks)) != len(shocks) or 0. not in shocks:
        raise ValueError('audit needs distinct shocks including zero, at most nine')
    if any(not math.isfinite(s) or abs(s) > 500 for s in shocks):
        raise ValueError('audit shocks must be finite and within +/-500 bp')
    tolerances = tolerances or {}
    if set(tolerances) - set(products): raise ValueError('tolerance product is outside audit scope')
    for product, fields in tolerances.items():
        if set(fields) - set(RISK + FLOWS): raise ValueError('unknown audit metric')
        for metric, tolerance in fields.items():
            if set(tolerance) - {'absolute', 'relative'}: raise ValueError('unknown tolerance field')
            _compare('', product, 0., 0, metric, 0., 0., tolerance)
    # Admission before any product computation; never silently sample records.
    cohorts = build['cohorts'].filter(pl.col('product').is_in(products))
    selected = build['lineage'].filter(pl.col('product').is_in(products))
    if not len(selected): raise ValueError('selected products have no loans')
    work = (len(selected)+len(cohorts))*max(config.n_paths, config.n_paths_base)*config.horizon*len(shocks)
    if work > 200_000_000: raise ValueError('audit exceeds 200 million position-path-month-scenario work units')
    position_books(build | {'cohorts': cohorts.head(1)}, products=products)
    deadline = time.perf_counter()+timeout
    def check():
        if cancelled and cancelled(): raise InterruptedError('cohort audit cancelled')
        if time.perf_counter() >= deadline: raise TimeoutError('cohort audit deadline exceeded')
    def publish(name, rows):
        check()
        if emit and rows: emit(name, pl.DataFrame(rows, infer_schema_length=None))
    summary = dict(loans=len(selected), cohorts=len(cohorts), checks=0, failed_checks=0,
        failed_cohorts=0, shocks_bp=shocks, cashflow_scenarios='base only',
        calculation_basis='individual_model_reprice', backend='rust',
        tolerances=tolerances, default_tolerance={'absolute': .01, 'relative': .01},
        calibration_validated=False, max_absolute_error={})
    # Each group retains at most 64 cohorts' monthly aggregate results. Loan
    # cashflows leave through the sink after every pricing batch.
    for group in cohorts.iter_slices(64):
        check()
        subset = build | {'cohorts': group}
        members = selected.filter(pl.col('cohort_id').is_in(group['cohort_id'].to_list()))
        rows = build['normalized'].filter(pl.col('loan_id').is_in(members['loan_id'].to_list()))
        subset['normalized'] = rows
        mapping = dict(members.select('loan_id', 'cohort_id').iter_rows())
        products_by_cohort = dict(group.select('cohort_id', 'product').iter_rows())
        reference, representative = defaultdict(float), defaultdict(float)
        def price(books, original):
            check()
            cache = DependencyCache(max_bytes=32*1024*1024, max_entries=10000)
            ids = {}
            for book, frame in books.items():
                for row in frame.iter_rows(named=True):
                    ident = row['cusip' if book == 'mbs' else 'id']
                    loan = row.get('source_loan_id')
                    ids[book, ident] = (mapping[loan] if original else row['cohort_id'], loan)
            target = reference if original else representative
            with run_context(config):
                for shock in shocks:
                    check()
                    result = price_books(books, asof=asof, swap_rates=swap_rates, vol_pts=vol_pts,
                        cache=cache, config=config, seed=seed, mbs_hists=mbs_hists, dep_hist=dep_hist,
                        scenario_market=(swap_rates + shock/10000., vol_pts),
                        include_analytics=True, include_key_rates=False)
                    emitted = []
                    for book, frame in result['positions'].items():
                        for r in frame.iter_rows(named=True):
                            cohort, loan = ids[book, r['id']]
                            for metric, source in [('market_value','market_value'), ('dv01','dv01'), ('nii','nii_total')]:
                                target[cohort, shock, 0, metric] += r[source]
                            if original:
                                emitted.append(dict(loan_id=loan, cohort_id=cohort, product=products_by_cohort[cohort],
                                    shock_bp=shock, market_value=r['market_value'], dv01=r['dv01'], nii=r['nii_total'],
                                    base_oas_bp=r['base_oas_bp'], calculation_basis='individual_model_reprice'))
                    publish('loan_risk', emitted)
                check()
                income = run_balance_sheet_nii(books | {'asof': asof, 'mbs_hists': mbs_hists},
                    swap_rates, vol_pts, dep_hist, horizon=config.horizon, seed=seed,
                    asof=asof, capture_cashflows=True)
                emitted = []
                for r in income['instrument_cashflows'].iter_rows(named=True):
                    cohort, loan = ids[r['book'], r['id']]
                    for metric in FLOWS: target[cohort, 0., r['month'], metric] += r[metric]
                    if original:
                        emitted.append(dict(loan_id=loan, cohort_id=cohort, month=r['month'],
                            **{m:r[m] for m in FLOWS}, calculation_basis='individual_model_reprice'))
                publish('loan_cashflows', emitted)
            cache.clear()
        representative_books = position_books(subset, products=products)
        for book, frame in representative_books.items():
            for batch in frame.iter_slices(batch_size): price({book:batch}, False)
        for batch in rows.sort('loan_id').iter_slices(batch_size):
            price(position_books(subset, products=products, loan_ids=batch['loan_id'].to_list()), True)
        if reference.keys() != representative.keys(): raise ArithmeticError('audit result key mismatch')
        errors, failed = [], set()
        for key in sorted(reference):
            cohort, shock, month, metric = key
            product = products_by_cohort[cohort]
            row = _compare(cohort, product, shock, month, metric, representative[key], reference[key],
                tolerances.get(product, {}).get(metric, {}))
            errors.append(row)
            summary['checks'] += 1
            summary['max_absolute_error'][metric] = max(summary['max_absolute_error'].get(metric, 0.), row['absolute_error'])
            if not row['passed']:
                failed.add(cohort); summary['failed_checks'] += 1
        summary['failed_cohorts'] += len(failed)
        publish('errors', errors)
        suggestions = []
        for cohort in sorted(failed):
            product = products_by_cohort[cohort]
            dims = {d['field']:d for d in build['config']['rules'][product]['dimensions'] if 'edges' in d}
            features = build['dispersion'].filter((pl.col('cohort_id') == cohort) & (pl.col('stddev') > 0))
            features = sorted(features.to_dicts(), key=lambda r: r['stddev']/max(abs(r['mean']),1e-12), reverse=True)
            candidates = [r for r in features if r['field'] in dims]
            if candidates:
                field = candidates[0]['field']
                values = rows.filter(pl.col('loan_id').is_in(members.filter(pl.col('cohort_id')==cohort)['loan_id'].to_list()))[field]
                edge = float(values.median())
                if edge in dims[field]['edges'] or not values.min() < edge < values.max(): continue
                suggestions.append(dict(cohort_id=cohort, product=product, field=field, proposed_edge=edge,
                    reason='Largest relative feature dispersion among numeric dimensions; rerun audit to validate improvement'))
        publish('suggestions', suggestions)
        if progress: progress(summary['checks'], summary['failed_checks'])
    check()
    summary['passed'] = summary['failed_checks'] == 0
    return summary
