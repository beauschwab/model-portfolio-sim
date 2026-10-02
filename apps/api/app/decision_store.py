"""Repository operations for the optional Rust decision prototype. No quant math."""
from contextlib import contextmanager
import time
import uuid

from . import store, persistence


def entry(sid, revision):
    with store._LOCK:
        persistence.assert_current(revision)
        if persistence.REPO is not None:
            saved = persistence.REPO.session(sid)
            if saved and saved['closed']:
                raise KeyError('decision session is closed')
        item = store.DECISIONS.get(sid)
        if item is None:
            raise KeyError('unknown or expired decision session')
        if revision != item['revision'] or revision != store.STATE_META['revision']:
            raise RuntimeError('REBUILD_REQUIRED: saved inputs changed; build a new decision session')
        item['accessed'] = time.monotonic()
        return item


def close(sid, *, durable=True):
    with store._LOCK:
        item = store.DECISIONS.pop(sid, None)
    if item:
        item['session'].close()
    saved = persistence.REPO.close_session(sid) if durable and persistence.REPO is not None else False
    return {'closed': bool(item) or saved}


def build(options):
    import copy
    options = copy.deepcopy(options)
    from portfolio_risk.strategy.decision import DecisionSession
    from portfolio_risk.core.runtime import RunConfig
    state = store.current_state()
    with store._LOCK:
        expired = [sid for sid, item in store.DECISIONS.items()
                   if time.monotonic() - item['accessed'] > 1800 or item['revision'] != store.STATE_META['revision']
                   or (persistence.REPO is not None and (persistence.REPO.session(sid) or {}).get('closed'))]
    for sid in expired:
        close(sid)
    with store._LOCK:
        if len(store.DECISIONS) >= 4:
            raise RuntimeError('Four decision sessions are open; close one before building another')
    settings = state['settings']
    settings.require_production_backend()
    config = RunConfig(settings.n_paths, settings.n_paths_base, settings.horizon_months,
                       state['assumptions'].get('deposit_segments'),
                       tuple(state['assumptions'].get('cd_ew_params', [])) or None, compute_backend=settings.compute_backend)
    sr, vp = state['market']['swap_rates'], state['market']['vol_pts']
    markets = [('base', sr, vp, 0.)]
    for name in options.pop('scenarios', []):
        s2, v2, spread = store.apply_scenario(state['scenarios'][name], 0)
        markets.append((name, s2, v2, spread))
    store.report(stage='building decision snapshot and strategy coefficients', pct=5.)
    session = DecisionSession(books=state['books'], asof=state['asof'], swap_rates=sr, vol_pts=vp,
        config=config, mbs_hists=state['mbs_hists'], dep_hist=state['dep_hist'], seed=settings.seed,
        extras={'mm': state['books'].get('mm'), 'hedges': state['hedges'], 'equity': state['equity']},
        constraints=options, markets=markets)
    sid = uuid.uuid4().hex
    try:
        store.report(stage='solving and independently replaying allocation', pct=90.)
        result = session.update(version=0)
        with store._LOCK:
            persistence.assert_current(state['revision'])
            if state['revision'] != store.STATE_META['revision']:
                raise RuntimeError('inputs changed during build; rebuild the decision session')
            store.DECISIONS[sid] = {'session': session, 'revision': state['revision'], 'accessed': time.monotonic()}
        return result | {'session_id': sid, 'revision': state['revision'],
                         'units': session.libraries[0]['units'], 'constraints': options}
    except BaseException:
        session.close()
        raise


def update(sid, request):
    item = entry(sid, request['expected_revision'])
    @contextmanager
    def publication():
        with store._LOCK:
            persistence.assert_current(item['revision'])
            if store.DECISIONS.get(sid) is not item or store.STATE_META['revision'] != item['revision']:
                raise RuntimeError('inputs changed or session closed during calculation; candidate discarded')
            yield
    store.report(stage='updating affected dependencies and solving', pct=10.)
    result = item['session'].update(version=request['version'], edits=request['edits'],
        templates=request['templates'], constraints=request['constraints'], publish_guard=publication)
    return result | {'session_id': sid, 'revision': item['revision']}


def evaluate(sid, request):
    item = entry(sid, request.expected_revision)
    result = item['session'].evaluate([a.model_dump() for a in request.allocation], request.version)
    entry(sid, request.expected_revision)  # saved input edits during evaluation invalidate response
    return result | {'session_id': sid, 'revision': item['revision']}
