"""Exact-cut publication binding for the bounded native fixed commercial route.

Opening amounts remain native outputs checked by independent replay. Only an
ordinary completed cut job in the current repository scope can be a predecessor.
"""
from copy import deepcopy
from . import commercial_prior_source as shared
from .completed_source_reader import encoded

SCHEMA = 'commercial-cut-mixed-journal-1'


def capture(repo, codec, source_job, cut_ordinal, check):
    return shared.capture(repo, codec, source_job, cut_ordinal, check, schema=SCHEMA)


def materialize(captured, revised_candidate, cut_ordinal):
    raw = captured['raw']
    if type(cut_ordinal) is not int or cut_ordinal != raw['cut_ordinal']:
        raise ValueError('exact published commercial cut required')
    result = dict(schema=SCHEMA, cut_ordinal=cut_ordinal,
        completed_candidate=deepcopy(raw['revised_candidate']),
        revised_candidate=deepcopy(revised_candidate))
    if len(encoded(result)) > shared.MAX_RAW - 1024:
        raise ValueError('bounded server-bound commercial cut input')
    return result


def _prefix(native):
    if native.get('transport') != 'fixed-sequential-cut-1':
        raise ValueError('verified fixed sequential cut source required')
    raw = native['raw_source']
    origin = raw['completed_candidate']['as_of_ordinal']
    cut = raw['cut_ordinal'] - origin
    # The independently replayed opening journal retains prior P&L in its GL.
    # Day-cut loan snapshots retain unweighted gross rights even for
    # zero participation. Future-only tables do not change this identity.
    tables = {}
    for name, rows in native['cut_tables'].items():
        prefix = [r for r in rows if type(r.get('day')) is int and r['day'] <= cut]
        if prefix:
            tables[name] = prefix
    return dict(origin_asof_ordinal=origin, cut_ordinal=raw['cut_ordinal'],
        cut_day=cut, tables=tables)


def _verify(raw, native, check):
    from portfolio_risk.analytics.commercial_fixed_capacity_reference import verify
    return verify(raw, native, check)


def _receipt(provenance):
    return dict(schema='commercial-cut-prior-opening-verification-1',
        modeled_prior_publication_verified=True, source_raw_and_journal_recomputed=True,
        exact_published_cut=True, published_cut_opening_matched=True,
        gross_unweighted_loan_cut_matched=True, separate_fee_adoption=False,
        completed_candidate_bound_to_prior_revised=True,
        cut_started_mixed_sim_adoption=True, observed_bank_reconciliation=False,
        general_mixed_book_adoption=False, empirical_validation=False,
        provenance=deepcopy(provenance))


def reopen(repo, codec, captured, check):
    import sys
    return shared.reopen(repo, codec, captured, check, schema=SCHEMA,
        _adapter=sys.modules[__name__])
