"""Static source binding review only; never loads or executes financial code."""
from pathlib import Path
import hashlib

# .gitattributes fixes the reviewed compiled inputs to LF on every platform.
# Keep the digest strict: other source edits require a new reviewed binding.
GENERATOR_FILES=('mortgage_state.rs','mortgage_receipts.rs','consumer_contract.rs','credit_operator.rs','credit.rs','amortization.rs','cashflow.rs','floating_rate.rs','limits.rs')
# Logical labels remain stable while compiled files move by domain.
GENERATOR_PATHS={'mortgage_state.rs': 'products/mortgages/state.rs',
 'mortgage_receipts.rs': 'products/mortgages/receipts.rs',
 'consumer_contract.rs': 'products/consumer/contracts.rs',
 'credit_operator.rs': 'models/credit/operator.rs',
 'credit.rs': 'models/credit/transition.rs',
 'amortization.rs': 'cashflows/amortization.rs',
 'cashflow.rs': 'cashflows/dated.rs',
 'floating_rate.rs': 'conventions/floating_rate.rs',
 'limits.rs': 'validation/limits.rs'}
EXPECTED_DIGEST='c07d2a5bf213c5d3860a722ce27079bd4a10120b1fc7e3b62118437a51571e03'
EXPECTED_BYTES=141787


def check_generator_copy(source_dir):
    root=Path(source_dir);digest=hashlib.sha256();count=0
    for name in GENERATOR_FILES:
        label=name.encode('utf-8');body=(root/GENERATOR_PATHS[name]).read_bytes()
        digest.update(len(label).to_bytes(8,'little'));digest.update(label)
        digest.update(len(body).to_bytes(8,'little'));digest.update(body)
        count+=16+len(label)+len(body)
    if digest.hexdigest()!=EXPECTED_DIGEST or count!=EXPECTED_BYTES:
        raise ArithmeticError('copied financial generator source binding')
    return dict(financial_source_sha256=digest.hexdigest(),generator_code_hash_bytes=count)
