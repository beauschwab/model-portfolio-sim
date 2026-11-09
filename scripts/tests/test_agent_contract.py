"""The contract budget counts physical lines, including blanks, on either platform."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("agent_contract", ROOT / "scripts/check_agent_contract.py")
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


class AgentContractTests(unittest.TestCase):
    def test_line_budget(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for newline in ("\n", "\r\n"):
                for trailing in (False, True):
                    for count in (149, 150, 151):
                        with self.subTest(newline=newline, trailing=trailing, count=count):
                            text = newline.join(["rule", *([""] * (count - 2)), "rule"])
                            if trailing:
                                text += newline
                            (root / "AGENTS.md").write_bytes(text.encode("utf-8"))
                            errors = checker.violations(root)
                            self.assertEqual(bool(errors), count > 150)
                            if errors:
                                self.assertIn("151 lines", errors[0])
                                self.assertIn("docs/changelog/agents-history.md", errors[0])

    def test_missing_contract_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            self.assertIn("missing", checker.violations(Path(tmp))[0])
