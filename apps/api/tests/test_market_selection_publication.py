"""Selected training stress rows must survive independent persisted replay."""
from copy import deepcopy

import pytest

from app import model_jobs, model_routes, persistence as db, worker
from app.model_tables import format_sources
from app.model_report_metadata import source_summary
from portfolio_risk.analytics.model_contracts import fit_market_drivers
from test_model_runs import queued
from test_persistence import durable


@pytest.mark.parametrize('selected', [[], [29, 29], [15, 14], [29], [39, 38]])
@pytest.mark.parametrize('phase', ['native', 'persisted'])
def test_changed_selection_cannot_publish(durable, monkeypatch, selected, phase):
    raw = model_routes.example('market-driver-calibration-1')['request']
    changed = fit_market_drivers(raw)
    changed['diagnostics']['selected'] = selected
    jid, job = queued('market-driver-calibration-1')
    if phase == 'native':
        original = model_jobs.native_result
        def replace(*args, **kwargs):
            _, execution = original(*args, **kwargs)
            return deepcopy(changed), execution
        monkeypatch.setattr(model_jobs, 'native_result', replace)
    else:
        original = db.CODEC.dump
        def replace(value):
            if isinstance(value, dict) and 'model_result' in value:
                formatted, layouts = format_sources(deepcopy(changed))
                value['model_result'] = formatted
                value['execution']['table_layouts'] = layouts
                value['execution']['summary'] = source_summary(changed)
                value['execution']['source_verification']['selected_count'] = len(selected)
            return original(value)
        monkeypatch.setattr(db.CODEC, 'dump', replace)
    worker.execute(job)
    row = db.REPO.job(jid)
    assert row['status'] == 'error' and row['result'] is None
    assert 'selected scenarios' in row['detail']


def test_market_verification_retains_original_cancellation():
    raw = model_routes.example('market-driver-calibration-1')['request']
    result = fit_market_drivers(raw)
    for stop in (1, 3, 10):
        calls = [0]
        def check():
            calls[0] += 1
            return calls[0] == stop
        with pytest.raises(InterruptedError, match='verification cancelled'):
            model_jobs.verify_source(dict(schema=raw['schema'], request=raw), result, check=check)
