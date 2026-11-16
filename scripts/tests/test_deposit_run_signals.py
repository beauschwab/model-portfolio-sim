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


if __name__ == '__main__':
    unittest.main()
