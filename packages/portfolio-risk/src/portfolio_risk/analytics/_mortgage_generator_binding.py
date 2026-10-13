"""Static source binding review only; never loads or executes financial code."""
from pathlib import Path
import hashlib

GENERATOR_FILES=('mortgage_state.rs','mortgage_receipts.rs','consumer_contract.rs','credit_operator.rs','credit.rs','amortization.rs','cashflow.rs','floating_rate.rs','limits.rs')
EXPECTED_DIGEST='cb467bb6260f6882d84a55a335c7bc91d377e228791b09a8a38d68c79d2e583a'
EXPECTED_BYTES=141669


def check_generator_copy(source_dir):
    root=Path(source_dir);digest=hashlib.sha256();count=0
    for name in GENERATOR_FILES:
        label=name.encode('utf-8');body=(root/name).read_bytes()
        digest.update(len(label).to_bytes(8,'little'));digest.update(label)
        digest.update(len(body).to_bytes(8,'little'));digest.update(body)
        count+=16+len(label)+len(body)
    if digest.hexdigest()!=EXPECTED_DIGEST or count!=EXPECTED_BYTES:
        raise ArithmeticError('copied financial generator source binding')
    return dict(financial_source_sha256=digest.hexdigest(),generator_code_hash_bytes=count)
