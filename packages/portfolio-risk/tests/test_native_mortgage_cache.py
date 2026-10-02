"""Native stage identity, reuse and edited-run equivalence to a cleared cache."""
import numpy as np
import polars as pl
import pytest

from portfolio_risk.core.quant_native import mortgage_cache_statistics as stats
from portfolio_risk.core.native import library_path
from test_native_mortgage_risk_owner import inputs, run, equal_frames

pytestmark = pytest.mark.skipif(not library_path().is_file(),reason='build native product backend')


def test_repeated_and_notional_edited_runs_reuse_native_market_stages():
    data = inputs(3)
    stats(clear=True)
    first = run(data,'rust',paths=1)
    cold = stats()
    repeated = run(data,'rust',paths=1)
    warm = stats()
    assert first.equals(repeated)
    assert warm['misses'] == cold['misses'] and warm['hits'] > cold['hits']
    assert 0 < warm['bytes'] <= 128*1024*1024
    book = data[0].with_columns((pl.col('current_face')*1.01).alias('current_face'))
    edited = (book,*data[1:])
    actual = run(edited,'rust',paths=1)
    assert stats()['misses'] == cold['misses']
    equal_frames(run(edited,'python',paths=1),actual)
    np.testing.assert_allclose(actual['dv01'],first['dv01']*1.01,rtol=1e-12,atol=1e-8)


@pytest.mark.parametrize('changed',['rates','quotes','cc_history','ps_history','seed','paths',
                                  'hpi_mu','hpi_beta','hpi_sigma','lag','cc_vol_points','path_dtype'])
def test_changed_stage_inputs_match_cold_rebuild(changed, monkeypatch):
    from portfolio_risk.core import config as cfg
    data = inputs(2)
    stats(clear=True)
    run(data,'rust',paths=1)
    before = stats()
    book,rates,quotes,cc,ps = data
    seed, paths = 19, 1
    if changed == 'rates':
        rates = rates.copy(); rates[3] += .0005
    elif changed == 'quotes':
        quotes = quotes.copy(); quotes[2,2] += .005
    elif changed == 'cc_history':
        cc = cc.with_columns((pl.col('cc') + .001).alias('cc'))
    elif changed == 'ps_history':
        ps = ps.with_columns((pl.col('ps') + .001).alias('ps'))
    elif changed == 'seed':
        seed = 2**64 + 27
    elif changed == 'paths':
        paths = 3
    else:
        settings = dict(hpi_mu=('HPI_MU',.04), hpi_beta=('HPI_BETA',-2.),
                        hpi_sigma=('HPI_SIG',.06), lag=('INC_LAG',4), path_dtype=('ADT',np.float64))
        if changed == 'cc_vol_points':
            value = np.asarray(cfg.CC_VOL_POINTS).copy(); value[0,0] += .25
            monkeypatch.setattr(cfg,'CC_VOL_POINTS',value)
        else:
            name,value = settings[changed]
            monkeypatch.setattr(cfg,name,value)
    edited = (book,rates,quotes,cc,ps)
    actual = run(edited,'rust',paths=paths,seed=seed)
    assert stats()['misses'] > before['misses']
    stats(clear=True)
    rebuilt = run(edited,'rust',paths=paths,seed=seed)
    assert actual.equals(rebuilt)


def test_failed_book_does_not_poison_valid_cached_run():
    data = inputs(2)
    stats(clear=True)
    first = run(data,'rust',paths=1)
    invalid = (data[0].with_columns(pl.lit(-1.).alias('price')),*data[1:])
    with pytest.raises(ValueError,match='no fallback'):
        run(invalid,'rust',paths=1)
    assert first.equals(run(data,'rust',paths=1))
