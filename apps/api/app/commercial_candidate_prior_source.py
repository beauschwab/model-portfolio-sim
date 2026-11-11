"""Server-selected prior publication for complete-origin candidate loan/fee windows.

Only completed ordinary sources at their exact published cut are admissible.
The shared reader and sealed SQL/artifact checks retain their historical bounds.
"""
from copy import deepcopy
from . import commercial_prior_source as shared
from .completed_source_reader import encoded

SCHEMA='commercial-candidate-holding-journal-1'


def capture(repo,codec,source_job,cut_ordinal,check):
    return shared.capture(repo,codec,source_job,cut_ordinal,check,schema=SCHEMA)


def materialize(captured,revised_candidate,cut_ordinal):
    raw=captured['raw']
    if type(cut_ordinal)is not int or cut_ordinal!=raw['cut_ordinal']:
        raise ValueError('exact published candidate opening date required')
    result=dict(schema=SCHEMA,cut_ordinal=cut_ordinal,
        completed_candidate=deepcopy(raw['revised_candidate']),revised_candidate=deepcopy(revised_candidate))
    if len(encoded(result))>shared.MAX_RAW-1024:raise ValueError('bounded server-bound candidate holding input')
    return result


def _prefix(native):
    cut=native['cut_day']
    # These are unweighted source-owner snapshots as well as actual postings.
    # Even zero-allocation sources retain their gross rights and acquired basis.
    tables=native['revised_candidate']['tables']
    return dict(origin_asof_ordinal=native['origin_asof_ordinal'],cut_ordinal=native['cut_ordinal'],cut_day=cut,
        opening=native['window_tables']['opening'],retained_prior_pnl=native['window_tables']['retained_prior_pnl'],
        source_tables={name:[r for r in rows if type(r.get('day'))is int and r['day']<=cut]
            for name,rows in tables.items()})


def _verify(raw,native,check):
    from portfolio_risk.analytics.commercial_candidate_holding_reference import verify
    return verify(raw,native,check)


def _receipt(provenance):
    return dict(schema='commercial-candidate-prior-opening-verification-1',modeled_prior_publication_verified=True,
        source_raw_and_journal_recomputed=True,exact_published_cut=True,completed_postings_and_opening_matched=True,
        gross_unweighted_loan_and_fee_prefix_matched=True,observed_bank_reconciliation=False,
        cut_started_mixed_sim_adoption=False,secondary_acquisition_adoption=False,empirical_validation=False,
        provenance=deepcopy(provenance))


def reopen(repo,codec,captured,check):
    import sys
    return shared.reopen(repo,codec,captured,check,schema=SCHEMA,_adapter=sys.modules[__name__])
