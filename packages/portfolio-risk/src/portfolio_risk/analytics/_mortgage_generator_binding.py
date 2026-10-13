"""Static source binding review only; never loads or executes financial code."""
from pathlib import Path
import hashlib

# .gitattributes fixes the reviewed compiled inputs to LF on every platform.
# Keep the digest strict: other source edits require a new reviewed binding.
GENERATOR_FILES=('mortgage_state.rs','mortgage_receipts.rs','consumer_contract.rs','credit_operator.rs','credit.rs','amortization.rs','cashflow.rs','floating_rate.rs','limits.rs')
EXPECTED_DIGEST='a925fcc8359f9a202dff5eaa75e4b9c5815467d6aba96701ca278eac515eca7f'
EXPECTED_BYTES=141631


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
