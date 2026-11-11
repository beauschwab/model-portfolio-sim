"""Golden-bank suite: committed inputs, comparison rules and gate-draft scoring.

These run without the engine. The end-to-end run itself is `make golden`.
"""
import json
import sys
import tempfile
import unittest
from decimal import Decimal
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2] / 'evals' / 'golden_banks'
sys.path.insert(0, str(ROOT))
import generate  # noqa: E402
import harness  # noqa: E402
import suite  # noqa: E402

BANKS = sorted(p.name for p in (ROOT / 'banks').iterdir() if (p / 'bank.json').exists())


class Inputs(unittest.TestCase):
    def test_committed_extracts_are_exactly_what_the_generator_produces(self):
        self.assertGreaterEqual(len(BANKS), 2)
        for bank in BANKS:
            for name, content in generate.files(bank).items():
                self.assertEqual((ROOT / 'banks' / bank / name).read_bytes(), content.encode(), f'{bank}/{name}; rerun generate.py')

    def test_trial_balance_balances_and_ties_to_the_declared_size(self):
        for bank in BANKS:
            spec = json.loads((ROOT / 'banks' / bank / 'bank.json').read_text(encoding='utf-8'))
            rows = suite.table(ROOT / 'banks' / bank / 'trial_balance.csv')
            side = lambda s: sum(Decimal(r['balance']) for r in rows if r['side'] == s)
            self.assertEqual(side('asset'), side('liability') + side('equity'))
            only = sum(Decimal(str(i['balance'])) for i in spec['generator'].get('gl_only', []) if i['side'] == 'asset')
            self.assertEqual(side('asset') - only, Decimal(spec['generator']['total_assets']))
            self.assertEqual({i['gl_account']: float(i['balance']) for i in spec['generator'].get('gl_only', [])},
                             spec['reconciliation']['expected_breaks'])

    def test_goldens_exist_and_record_a_clean_reconciliation(self):
        for bank in BANKS:
            golden = json.loads((ROOT / 'banks' / bank / 'expected.json').read_text(encoding='utf-8'))
            self.assertEqual(golden['reconciliation.unexplained_accounts'][0], 0)
            self.assertIs(golden['intake.sha256_matches_extract'][0], True)
            self.assertIs(golden['optimize.feasible'][0], True)
            for area in ('cohorts.', 'publication.', 'risk.', 'nii.', 'stress.', 'kpis.', 'scenario_nii.', 'optimize.', 'strategy_eval.'):
                self.assertTrue(any(k.startswith(area) for k in golden), f'{bank} has no {area} metrics')


class Comparison(unittest.TestCase):
    def test_model_outputs_allow_only_roundoff(self):
        golden = {'risk.mbs.dv01.sum': [-1000., 5000.]}
        self.assertEqual(suite.compare(golden, {'risk.mbs.dv01.sum': [-1000.0004, 5000.]}), [])
        self.assertEqual(len(suite.compare(golden, {'risk.mbs.dv01.sum': [-1000.01, 5000.]})), 1)

    def test_conservation_metrics_are_exact_to_double_rounding(self):
        golden = {'publication.books.mbs.balance': [1e10, 1e10]}
        self.assertEqual(len(suite.compare(golden, {'publication.books.mbs.balance': [1e10 + 1., 1e10]})), 1)

    def test_missing_new_changed_text_and_nonfinite_values_are_differences(self):
        golden = {'a.rows': [3, None], 'b': ['rust', None], 'c': [1., 1.], 'gone': [1., 1.]}
        actual = {'a.rows': [4, None], 'b': ['python', None], 'c': [float('nan'), 1.], 'new': [1., 1.]}
        self.assertEqual({d[0] for d in suite.compare(golden, actual)}, {'a.rows', 'b', 'c', 'gone', 'new'})

    def test_run_dependent_solver_counters_are_not_golden(self):
        self.assertTrue(suite.ignored('optimize.solver.simplex_iterations'))
        self.assertFalse(suite.ignored('optimize.worst_case_nii_$'))


class GateScoring(unittest.TestCase):
    def drafts(self, bank):
        pod = Path(tempfile.mkdtemp())
        harness.reference(bank, pod)
        return pod

    def edit(self, pod, gate, change):
        path = pod / 'evidence' / 'gates' / f'{gate}.json'
        record = json.loads(path.read_text(encoding='utf-8'))
        change(record)
        path.write_text(json.dumps(record), encoding='utf-8')

    def test_reference_drafts_score_full_marks(self):
        for bank in BANKS:
            result = harness.score(bank, self.drafts(bank))
            self.assertTrue(result['passed'], result)
            self.assertEqual(result['overall'], 1.)

    def test_self_approval_scores_zero(self):
        pod = self.drafts('harbor-120b')
        self.edit(pod, 'G3-reconciled', lambda r: r.update(decision='approved', approver='agent'))
        result = harness.score('harbor-120b', pod)
        self.assertEqual(result['gates']['G3-reconciled']['score'], 0.)
        self.assertFalse(result['passed'])

    def test_missed_break_wrong_amount_and_invented_exception_fail(self):
        for change in (lambda r: r['exceptions'].pop(),
                       lambda r: r['exceptions'][0].update(amount_usd=1.),
                       lambda r: r['exceptions'].extend([dict(id='x', description='unrelated', proposed_disposition='open')] * 4)):
            pod = self.drafts('harbor-120b')
            self.edit(pod, 'G3-reconciled', change)
            self.assertLess(harness.score('harbor-120b', pod)['gates']['G3-reconciled']['score'], harness.PASS)

    def test_clean_bank_penalises_invented_breaks_and_missing_or_malformed_drafts(self):
        pod = self.drafts('prairie-20b')
        self.edit(pod, 'G3-reconciled', lambda r: r['exceptions'].append(dict(id='b', description='break 1300', amount_usd=5., proposed_disposition='open')))
        self.edit(pod, 'G2-mapping-approved', lambda r: r.pop('evidence'))
        (pod / 'evidence' / 'gates' / 'G1-data-accepted.json').unlink()
        gates = harness.score('prairie-20b', pod)['gates']
        self.assertLess(gates['G3-reconciled']['score'], harness.PASS)
        self.assertEqual(gates['G2-mapping-approved']['score'], 0.)
        self.assertEqual(gates['G1-data-accepted']['findings'], ['no draft'])

    def test_emitted_pod_carries_the_extracts_and_task(self):
        pod = harness.emit('prairie-20b', Path(tempfile.mkdtemp()) / 'pod')
        for name in ('pod.yaml', 'TASK.md', 'data/trial_balance.csv', 'data/extracts/retail_tape.csv', 'evidence/gates/.gitkeep'):
            self.assertTrue((pod / name).exists(), name)


if __name__ == '__main__':
    unittest.main()
