"""Exact reviewed source binding survives checkout portability, not source edits."""
from pathlib import Path
import shutil

import pytest

from portfolio_risk.analytics import _mortgage_generator_binding as binding
from portfolio_risk.analytics import mortgage_credit_math_reference as model


SOURCE = Path(__file__).resolve().parents[2] / 'portfolio-model-core' / 'src'


def test_reviewed_checkout_bytes_match_independent_binding():
    for name in binding.GENERATOR_FILES:
        assert b'\r' not in (SOURCE / name).read_bytes(), name
    actual = binding.check_generator_copy(SOURCE)
    assert actual == dict(financial_source_sha256=model.SOURCE_SHA,
                         generator_code_hash_bytes=model.GENERATOR_CODE_HASH_BYTES)


@pytest.mark.parametrize('change', ['line_endings', 'same_size_source_edit'])
def test_changed_source_bytes_still_reject(tmp_path, change):
    for name in binding.GENERATOR_FILES:
        shutil.copyfile(SOURCE / name, tmp_path / name)
    path = tmp_path / binding.GENERATOR_FILES[0]
    body = path.read_bytes()
    if change == 'line_endings':
        body = body.replace(b'\n', b'\r\n')
    else:
        body = bytes([body[0] ^ 1]) + body[1:]
    path.write_bytes(body)
    with pytest.raises(ArithmeticError, match='source binding'):
        binding.check_generator_copy(tmp_path)
