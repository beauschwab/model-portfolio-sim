"""Verified same/later-cut chains for the bounded native fixed commercial route.

Opening amounts remain native outputs checked by independent replay. Only an
completed cut job in the current repository scope can be a predecessor; every
ancestor is independently replayed under one cumulative budget.
"""
from copy import deepcopy
from . import commercial_prior_source as shared
from .completed_source_reader import encoded, exact

SCHEMA = 'commercial-cut-mixed-journal-1'


def capture(repo, codec, source_job, cut_ordinal, check):
    captured = shared.capture(repo, codec, source_job, cut_ordinal, check, schema=SCHEMA,
        _allow_prior=True, _allow_later=True)
    if cut_ordinal != captured['raw']['cut_ordinal']:
        captured['target_cut_ordinal'] = cut_ordinal
    return captured


def materialize(captured, revised_candidate, cut_ordinal):
    raw = captured['raw']
    target = captured.get('target_cut_ordinal', raw['cut_ordinal'])
    if type(cut_ordinal) is not int or cut_ordinal != target or cut_ordinal < raw['cut_ordinal']:
        raise ValueError('server-selected published or later commercial cut required')
    result = dict(schema=SCHEMA, cut_ordinal=cut_ordinal,
        completed_candidate=deepcopy(raw['revised_candidate']),
        revised_candidate=deepcopy(revised_candidate))
    if len(encoded(result)) > shared.MAX_RAW - 1024:
        raise ValueError('bounded server-bound commercial cut input')
    return result


def _prefix(native):
    from portfolio_risk.analytics.commercial_cut_prior_reference import prefix
    return prefix(native)


def _verify(raw, native, check):
    from portfolio_risk.analytics.commercial_fixed_capacity_reference import verify
    return verify(raw, native, check)


def _receipt(provenance):
    receipt = dict(schema='commercial-cut-prior-opening-verification-1',
        modeled_prior_publication_verified=True, source_raw_and_journal_recomputed=True,
        exact_published_cut=True, published_cut_opening_matched=True,
        gross_unweighted_loan_cut_matched=True, separate_fee_adoption=False,
        completed_candidate_bound_to_prior_revised=True,
        cut_started_mixed_sim_adoption=True, observed_bank_reconciliation=False,
        general_mixed_book_adoption=False, empirical_validation=False,
        provenance=deepcopy(provenance))

    if 'matched_cut_ordinal' in provenance or 'ancestor_verification' in provenance:
        target = provenance.get('matched_cut_ordinal', provenance['cut_ordinal'])
        receipt.update(schema='commercial-cut-prior-opening-verification-2',
            exact_published_cut=target == provenance['cut_ordinal'],
            published_cut_opening_matched=target == provenance['cut_ordinal'],
            published_journal_cut_opening_matched=True,
            published_cut_ordinal=provenance['cut_ordinal'], matched_cut_ordinal=target,
            published_journal_rolled_forward=target != provenance['cut_ordinal'],
            verified_publications=1 + provenance.get('ancestor_verification', {}).get('verified_publications',
                1 if 'ancestor_verification' in provenance else 0))
    return receipt


class _CutAdapter:
    def __init__(self, captured, check):
        self.target = captured.get('target_cut_ordinal', captured['raw']['cut_ordinal'])
        self.original = captured['raw']['cut_ordinal']
        self.check = check

    def _verify(self, raw, native, check):
        return _verify(raw, native, check)

    _receipt = staticmethod(_receipt)

    def _prefix(self, native):
        from portfolio_risk.analytics.commercial_cut_prior_reference import prefix
        return prefix(native, self.target, self.check)

    def _match(self, native, expected, check):
        actual = self._prefix(native)
        if self.target == self.original:
            exact(actual, expected, 'published prior opening or completed postings differ')
        else:
            from portfolio_risk.analytics.commercial_cut_prior_reference import match_advanced
            match_advanced(expected, actual, check)

    def _parent(self, repo, codec, queued, check, chain):
        captured = queued['prior_publication']; raw = queued['request']
        expected = materialize(captured, raw['revised_candidate'], raw['cut_ordinal'])
        exact(expected, raw, 'ancestor server-bound raw candidate differs')
        return reopen(repo, codec, captured, check, _chain=chain)

    def _provenance(self, provenance, captured, parent):
        if self.target != self.original:
            provenance['matched_cut_ordinal'] = self.target
        if parent is not None:
            provenance['ancestor_verification'] = parent.receipt()


def reopen(repo, codec, captured, check, *, _chain=None):
    from .commercial_cut_prior_chain import ChainBudget
    chain = ChainBudget() if _chain is None else _chain
    return shared.reopen(repo, codec, captured, check, schema=SCHEMA,
        _adapter=_CutAdapter(captured, check), _chain=chain)
