"""Deliberate forbidden fixtures, without importing application/financial code."""
import importlib.util
import json
from pathlib import Path
import subprocess
import unittest

ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('architecture',ROOT/'scripts/check_architecture.py')
checker=importlib.util.module_from_spec(spec);spec.loader.exec_module(checker)


class ArchitectureTests(unittest.TestCase):
    def test_shared_model_sources_keep_exact_binding_at_canonical_paths(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / 'packages/core/src'
            domain = source / 'products/mortgages'
            domain.mkdir(parents=True)
            (root / 'docs/contracts').mkdir(parents=True)
            (source / 'lib.rs').write_text('pub use products::mortgages::state as mortgage_state;')
            (domain / 'state.rs').write_text('')
            receipt = domain / 'receipts.rs'
            valid = 'const FILES: &[(&str, &[u8])] = &[("mortgage_state.rs", include_bytes!("state.rs")), ("mortgage_receipts.rs", include_bytes!("receipts.rs"))];'
            receipt.write_text(valid)
            paths = {'mortgage_state.rs': 'products/mortgages/state.rs',
                     'mortgage_receipts.rs': 'products/mortgages/receipts.rs'}
            (root / 'docs/contracts/model-core-domains.json').write_text(json.dumps({
                'crate': 'packages/core', 'root_modules': ['lib.rs'],
                'relocations': paths,
                'compatibility_exports': {'mortgage_state': 'products::mortgages::state'},
                'exact_source_binding': {'module': paths['mortgage_receipts.rs'], 'files': paths},
            }))
            self.assertEqual(checker.native_domain_violations(root), [])
            receipt.write_text(valid.replace('include_bytes!("state.rs")', 'include_bytes!("../state.rs")'))
            self.assertIn('exact-source binding', '\n'.join(checker.native_domain_violations(root)))
            receipt.unlink()
            self.assertIn('must live only', '\n'.join(checker.native_domain_violations(root)))

    def test_native_domain_ratchet_and_math_direction(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / 'packages/fixture-native/src'
            (source / 'math').mkdir(parents=True)
            (root / 'docs/contracts').mkdir(parents=True)
            (root / 'docs/contracts/native-domains.json').write_text(json.dumps({
                'crate': 'packages/fixture-native', 'root_modules': ['lib.rs'],
                'relocations': {'random.rs': 'math/random.rs'},
                'compatibility_exports': {'random': 'math::random'},
                'nested_compatibility_exports': {'math/mod.rs': {'rng': 'self::random'}},
                'extractions': {'quant.rs': {'retired': True, 'destinations': ['math/arrays.rs']}},
            }))
            (source / 'lib.rs').write_text('pub mod math;\npub use math::random;')
            (source / 'math/mod.rs').write_text('pub use self::random as rng;')
            numerical = source / 'math/random.rs'
            numerical.write_text('use crate::math::ziggurat; // crate::products is documentation')
            (source / 'math/arrays.rs').write_text('')
            self.assertEqual(checker.native_domain_violations(root), [])
            (source / 'math/arrays.rs').unlink()
            self.assertIn('missing extracted domain source', '\n'.join(checker.native_domain_violations(root)))
            (source / 'math/arrays.rs').write_text('')
            (source / 'quant.rs').write_text('')
            self.assertIn('retired mixed module', '\n'.join(checker.native_domain_violations(root)))
            (source / 'quant.rs').unlink()
            (source / 'math/mod.rs').write_text('')
            self.assertIn('missing compatibility export rng', '\n'.join(checker.native_domain_violations(root)))
            (source / 'math/mod.rs').write_text('pub use self::random as rng;')
            for forbidden in ['use crate::products::Mortgage;', 'use crate::{market};', 'use std::fs;']:
                numerical.write_text(forbidden)
                self.assertTrue(checker.native_domain_violations(root))
            numerical.write_text('')
            (source / 'misc.rs').write_text('')
            self.assertIn('new flat native module', '\n'.join(checker.native_domain_violations(root)))
            (source / 'random.rs').write_text('')
            self.assertIn('must live only', '\n'.join(checker.native_domain_violations(root)))
            (source / 'lib.rs').write_text('pub mod math;')
            self.assertIn('missing compatibility', '\n'.join(checker.native_domain_violations(root)))

    def test_deferred_relative_handler(self):
        errors=checker.violations('app.persistence','def run():\n    from . import store\n')
        self.assertTrue(errors);self.assertIn('app.store',errors[0]);self.assertIn(':2:',errors[0])

    def test_engine_cannot_import_application(self):
        self.assertTrue(checker.violations('portfolio_risk.core.example','from app import store'))
        self.assertTrue(checker.violations('portfolio_risk.core.example','def x():\n    __import__("app.store")'))

    def test_mcp_cannot_bypass_http_boundary(self):
        for target in ['app.store', 'portfolio_risk.core.quant_native', 'sqlalchemy', 'sqlite3', 'ctypes']:
            self.assertTrue(checker.violations('workbench_mcp.adapter', 'import ' + target))
        self.assertFalse(checker.violations('workbench_mcp.adapter', 'import httpx\nimport pyarrow'))

    def test_storage_cannot_import_dispatch_or_features(self):
        for target in ['operations','pricing_jobs','decision_store','cohort_store','treasury_store','worker']:
            self.assertTrue(checker.violations('app.persistence','from . import '+target))

    def test_canonical_imports_and_comments(self):
        self.assertFalse(checker.violations('app.pricing_jobs','from portfolio_risk.analytics.risk import run_risk\n# from . import worker'))
        self.assertTrue(checker.violations('app.pricing_jobs','from portfolio_risk.risk import run_risk'))

    def test_long_cycles_are_detected(self):
        self.assertEqual(len(checker.cycle_edges({'a':{'b'},'b':{'c'},'c':{'a'}})),3)

    def test_rust_application_dependency(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            p=Path(tmp)/'packages/fixture-native';p.mkdir(parents=True)
            (p/'Cargo.toml').write_text('[dependencies]\napp = { path = "../../apps/api" }')
            self.assertTrue(checker.rust_violations(Path(tmp)))

    def test_typescript_ast_fixtures(self):
        subprocess.run(['node',str(ROOT/'scripts/check_web_boundaries.mjs'),'--self-test'],check=True)


if __name__=='__main__':unittest.main()
