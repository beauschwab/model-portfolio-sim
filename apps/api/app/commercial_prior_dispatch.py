"""Fixed server-owned selection of independently verified holding sources."""
from . import commercial_prior_source,commercial_candidate_prior_source,commercial_cut_prior_source


def source_for(schema):
    if schema==commercial_prior_source.SCHEMA:return commercial_prior_source
    if schema==commercial_candidate_prior_source.SCHEMA:return commercial_candidate_prior_source
    if schema==commercial_cut_prior_source.SCHEMA:return commercial_cut_prior_source
    raise ValueError('native holding prior binding required')
