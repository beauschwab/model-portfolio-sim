"""Change-based selection must cover every code area and keep the tiers wired to real files."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("select_tests", ROOT / "scripts/select_tests.py")
selector = importlib.util.module_from_spec(spec)
spec.loader.exec_module(selector)


class SelectionTests(unittest.TestCase):
    def test_every_code_area_selects_a_suite(self):
        for parent in ("apps", "packages"):
            for area in sorted(p for p in (ROOT / parent).iterdir() if p.is_dir()):
                with self.subTest(area=area.name):
                    suites, unmapped = selector.select([f"{parent}/{area.name}/src/changed"])
                    self.assertFalse(unmapped)
                    self.assertGreater(len(suites), 1)  # its own suite plus architecture
                    self.assertIn("architecture", suites)

    def test_shared_crates_select_their_dependents(self):
        suites, _ = selector.select(["packages/portfolio-model-core/src/lib.rs"])
        self.assertEqual(suites, ["architecture", "rust:model-core", "rust:ledger", "rust:risk",
                                  "rust:decision", "engine", "api"])
        suites, _ = selector.select(["packages/portfolio-verification-native/src/lib.rs"])
        self.assertEqual(suites, ["architecture", "rust:verification", "rust:decision", "rust:service"])

    def test_narrow_changes_stay_narrow(self):
        self.assertEqual(selector.select(["apps/web/src/pages/Home.tsx"]), (["architecture", "web"], False))
        self.assertEqual(selector.select(["apps\\mcp\\src\\workbench_mcp\\adapter.py"]), (["architecture", "mcp"], False))
        self.assertEqual(selector.select(["scripts/select_tests.py"]), (["architecture"], False))
        self.assertEqual(selector.select(["AGENTS.md"]), (["architecture"], False))

    def test_prose_selects_nothing_and_unmapped_paths_select_smoke(self):
        self.assertEqual(selector.select(["README.md", "docs/architecture.md"]), ([], False))
        self.assertEqual(selector.select(["docs/contracts/http.json"]), (["architecture"], False))
        for path in (".github/workflows/smoke.yml", "bun.lock", "turbo.json"):
            with self.subTest(path=path):
                self.assertEqual(selector.select([path]), ([], True))

    def test_rules_and_smoke_tests_name_real_targets(self):
        for prefix, suites in selector.RULES.items():
            self.assertTrue((ROOT / prefix).is_dir(), prefix)
            for name in suites or ():
                self.assertIn(name, selector.SUITES)
        for cwd, tests in selector.SMOKE_TESTS.items():
            for test in tests:
                self.assertTrue((ROOT / cwd / test).is_file(), f"{cwd}/{test}")

    def test_nightly_runs_every_workflow(self):
        workflows = ROOT / ".github/workflows"
        nightly = (workflows / "nightly.yml").read_text(encoding="utf-8")
        for path in sorted(workflows.glob("*.yml")):
            if path.name in ("nightly.yml", "smoke.yml"):
                continue
            with self.subTest(workflow=path.name):
                self.assertIn(f"uses: ./.github/workflows/{path.name}\n", nightly)
                self.assertIn("\n  workflow_call:", path.read_text(encoding="utf-8").replace("\r\n", "\n"))


if __name__ == "__main__":
    unittest.main()
