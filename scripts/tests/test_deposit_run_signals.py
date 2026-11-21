"""The deposit-run evidence study must not read 2023 data into its 2022 signals."""
import importlib.util
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
try:
    import polars as pl
except ImportError:          # the architecture check runs without the data stack
    pl = None


def load():
    spec = importlib.util.spec_from_file_location('evaluate_deposit_run_signals', ROOT/'scripts/evaluate_deposit_run_signals.py')
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def row(cert, quarter, deposits, uninsured=50, assets=20_000_000):
    return dict(CERT=cert, NAME=f'BANK {cert}', REPDTE=quarter, ASSET=assets, DEP=deposits, DEPDOM=100, DEPUNINS=uninsured,
                EQ=100, SCHA=50, SCHF=40, SCAF=50, SCAA=50, CHBAL=10)


@unittest.skipIf(pl is None, 'polars is not installed')
class DepositRunSignals(unittest.TestCase):
    def test_signals_refuse_post_cutoff_rows(self):
        module = load()
        with self.assertRaises(AssertionError):
            module.signals(pl.DataFrame([row(1, '20221231', 100), row(1, '20230331', 50)]))

    def test_auc_counts_ties_as_half(self):
        module = load()
        self.assertEqual(module.auc([3, 2, 1], [True, False, False]), 1.0)
        self.assertEqual(module.auc([1, 2, 3], [True, False, False]), 0.0)
        self.assertEqual(module.auc([2, 2, 1], [True, False, False]), 0.75)
        self.assertIsNone(module.auc([1, 2], [False, False]))

    def test_outcomes_come_only_from_the_failures_list_and_2023_rows(self):
        module = load()
        quarters = ['20191231', '20211231', '20221231']
        rows = [row(c, q, 100, uninsured=u) for c, u in ((1, 90), (2, 60), (3, 30), (4, 10)) for q in quarters]
        rows += [row(2, '20230331', 70), row(3, '20230331', 100)]          # bank 2 loses 30%; bank 4 files nothing
        result = module.evaluate(pl.DataFrame(rows), [dict(CERT=1, FAILDATE='3/10/2023'), dict(CERT=3, FAILDATE='7/28/2023')])
        self.assertEqual(result['evaluated'], 3)                           # bank 4 merged or closed: excluded, not a non-event
        self.assertEqual([b['name'] for b in result['excluded']], ['BANK 4'])
        self.assertEqual({b['name'] for b in result['event_banks']}, {'BANK 1', 'BANK 2'})   # bank 3 failed after the window
        uninsured = next(s for s in result['signals'] if s['signal'] == 'uninsured_share')
        self.assertEqual(uninsured['auc'], 1.0)
        self.assertEqual(uninsured['event_ranks'], {'BANK 1': 1, 'BANK 2': 2})

    def test_missing_equity_is_unscored_not_zero_in_the_combined_signal(self):
        module = load()
        rows = [row(1, '20221231', 100), dict(row(2, '20221231', 100), EQ=None), dict(row(3, '20221231', 100), SCHF=60)]
        table = {r['CERT']: r for r in module.signals(pl.DataFrame(rows)).iter_rows(named=True)}
        self.assertAlmostEqual(table[1]['securities_loss_to_equity'], 0.1)            # (50-40 + 50-50) / 100
        self.assertAlmostEqual(table[1]['uninsured_times_loss'], 0.05)                 # 50/100 uninsured x 0.1
        self.assertIsNone(table[2]['securities_loss_to_equity'])                      # no equity: no ratio
        self.assertIsNone(table[2]['uninsured_times_loss'])                           # and not a score of zero
        self.assertAlmostEqual(table[3]['securities_loss_to_equity'], -0.1)           # a net gain
        self.assertEqual(table[3]['uninsured_times_loss'], 0.)                         # floored at zero, still scored

    def test_predictive_bound_matches_the_t_table_and_covers_one_percent(self):
        module = load()
        # Hand check from printed t tables: t(0.99, 15) = 2.602, t(0.99, 7) = 2.998.
        self.assertAlmostEqual(module.predictive_bound(16, .01), -2.602*(17/16)**.5, places=3)   # -2.683
        self.assertAlmostEqual(module.predictive_bound(8, .01), -2.998*(9/8)**.5, places=3)      # -3.180
        import numpy as np
        rng = np.random.default_rng(7)
        draws = rng.standard_normal((200_000, 17))                  # 16 training quarters and one new one
        train, new = draws[:, :16], draws[:, 16]
        z = (new-train.mean(axis=1))/train.std(axis=1, ddof=1)
        self.assertAlmostEqual((z < module.predictive_bound(16, .01)).mean(), .01, delta=.001)
        self.assertGreater((z < -2.3263478740408408).mean(), .018)  # the normal quantile breaches about 2%, not 1%

    def test_coverage_is_reported_per_signal(self):
        module = load()
        quarters = ['20191231', '20211231', '20221231']
        rows = [row(c, q, 100, uninsured=u) for c, u in ((1, 90), (2, 60), (3, 30), (4, 10)) for q in quarters if not (c == 2 and q == '20191231')]
        rows += [row(2, '20230331', 70), row(3, '20230331', 100), row(4, '20230331', 100)]
        result = module.evaluate(pl.DataFrame(rows), [dict(CERT=1, FAILDATE='3/10/2023')])
        growth = next(s for s in result['signals'] if s['signal'] == 'deposit_growth_2019_2021')
        self.assertEqual((growth['banks'], growth['events'], growth['unscored_events']), (3, 1, ['BANK 2']))
        self.assertNotIn('BANK 2', growth['event_ranks'])
        uninsured = next(s for s in result['signals'] if s['signal'] == 'uninsured_share')
        self.assertEqual((uninsured['banks'], uninsured['events'], uninsured['unscored_events']), (4, 2, []))
        self.assertEqual(result['common_sample'], dict(banks=3, events=1, unscored_events=['BANK 2']))
        self.assertEqual(uninsured['common_auc'], 1.0)                 # banks 1, 3, 4 only


if __name__ == '__main__':
    unittest.main()
