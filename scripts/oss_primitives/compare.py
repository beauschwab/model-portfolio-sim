"""Reproduce the isolated, pinned Rust primitive comparison.

Run: uv run --project apps/api python scripts/oss_primitives/compare.py
Requires git, MSVC linking tools, and rustup toolchain 1.95.0.
Downloads source and builds outside the repository; never edits upstream code.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import statistics
import subprocess
import tempfile
import time
from types import SimpleNamespace

import numba
import numpy as np
from portfolio_risk.products.corp import corp_pv

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
PINS = {
    "convex": ("https://github.com/sujitn/convex.git", "acc357b8562587194a6c419bd38034b9d69dd62c"),
    "stochastic-rs": ("https://github.com/rust-dd/stochastic-rs.git", "2d6bcc7b42927b5b9777bf237ea5ca9ec102f1ac"),
    "finstack-quant": ("https://github.com/jeickmeier/finstack-quant.git", "19355232773b2c0daf81070d72a96171eebe0c23"),
}


def command(args, **kwargs):
    result = subprocess.run(args, text=True, capture_output=True, **kwargs)
    if result.returncode:
        raise RuntimeError(f"Command failed ({result.returncode}): {args}\n{result.stderr}")
    return result.stdout.strip()


def timed(function, inner=1):
    for _ in range(3):
        function()
    samples = []
    for _ in range(11):
        start = time.perf_counter()
        for _ in range(inner):
            result = function()
        samples.append((time.perf_counter() - start) * 1e6 / inner)
    return {"median_us": statistics.median(samples), "p95_us": max(samples),
            "samples_us": samples, "checksum": float(result)}


@numba.njit(parallel=True)
def fused_numba(amounts, times, offsets, rates):
    """Benchmark-only control, not an application change or OSS implementation."""
    result = np.empty(len(rates))
    for i in numba.prange(len(rates)):
        total = 0.0
        for j in range(offsets[i], offsets[i + 1]):
            total += amounts[j] * np.exp(-rates[i] * times[j])
        result[i] = total
    return result


def python_comparison(fixture, references, threads):
    numba.set_num_threads(threads)
    results = []
    for n in (1, 1000, 10000):
        rows = [np.asarray(fixture[i % 41], dtype=np.float64) for i in range(n)]
        counts = np.asarray([len(row) for row in rows], dtype=np.int64)
        offsets = np.r_[0, counts.cumsum()]
        flat = np.concatenate(rows)
        times = np.ascontiguousarray(flat[:, 0])
        amounts = np.ascontiguousarray(flat[:, 1])
        # Existing CSR pricing assumes rate-path discounting is already in A.
        # Exactly one deterministic path, 4% continuous base rate + 1% OAS.
        a = amounts * np.exp(-0.04 * times)
        oas = np.full(n, 0.01)
        deck = SimpleNamespace(per_off=offsets, t_pay=times)
        expected = np.asarray([references[i % 41] for i in range(n)])
        actual = corp_pv(deck, a, oas, 1)
        np.testing.assert_allclose(actual, expected, rtol=0, atol=1e-9)
        compiled = fused_numba(amounts, times, offsets, np.full(n, 0.05))
        np.testing.assert_allclose(compiled, expected, rtol=0, atol=1e-9)
        inner = 100 if n == 1 else 1
        rate_vector = np.full(n, 0.05)
        row = {"instruments": n,
               "existing_corp_pv_prepared_A": timed(lambda: corp_pv(deck, a, oas, 1).sum(), inner),
               "custom_fused_numba": timed(lambda: fused_numba(amounts, times, offsets, rate_vector).sum(), inner),
               "max_abs_error": float(np.max(np.abs(actual - expected))),
               "prepared_array_bytes": int(a.nbytes + times.nbytes + offsets.nbytes + oas.nbytes)}
        # Selective edit diagnostic: change one OAS, leave base calibration frozen.
        # This measures an adapter, not a dependency graph or product rebuild.
        idx = n // 2
        edited = oas.copy()
        edited[idx] += 0.0025
        full = corp_pv(deck, a, edited, 1)
        lo, hi = offsets[idx:idx + 2]
        small_deck = SimpleNamespace(per_off=np.array([0, hi - lo]), t_pay=times[lo:hi])
        prior_total = actual.sum()

        def selective():
            replacement = corp_pv(small_deck, a[lo:hi], edited[idx:idx + 1], 1)[0]
            return prior_total - actual[idx] + replacement

        np.testing.assert_allclose(selective(), full.sum(), rtol=1e-14, atol=1e-9)
        row["one_oas_edit_full_reprice"] = timed(lambda: corp_pv(deck, a, edited, 1).sum(), inner)
        row["one_oas_edit_selective_adapter"] = timed(selective, 100)
        results.append(row)
    return results


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-root", type=Path,
                        default=Path(tempfile.gettempdir()) / "rates-workbench-oss-20260928")
    parser.add_argument("--output", type=Path,
                        default=REPO / "docs/reviews/2026-09-28-oss-primitives.json")
    args = parser.parse_args()
    root = args.source_root.resolve()
    root.mkdir(parents=True, exist_ok=True)
    for name, (url, revision) in PINS.items():
        path = root / name
        if not path.exists():
            command(["git", "init", str(path)])
            command(["git", "-C", str(path), "remote", "add", "origin", url])
            command(["git", "-C", str(path), "fetch", "--depth", "1", "origin", revision])
            command(["git", "-C", str(path), "checkout", "--detach", "FETCH_HEAD"])
        if command(["git", "-C", str(path), "rev-parse", "HEAD"]) != revision:
            raise RuntimeError(f"{name}: expected pinned revision {revision}; use a fresh --source-root")
        if command(["git", "-C", str(path), "status", "--porcelain"]):
            raise RuntimeError(f"{name}: source tree has local changes")
    probe = root / "probe"
    (probe / "src").mkdir(parents=True, exist_ok=True)
    (probe / "Cargo.toml").write_text(
        (HERE / "Cargo.toml.in").read_text().replace("@ROOT@", root.as_posix()), encoding="utf-8")
    shutil.copyfile(HERE / "main.rs", probe / "src/main.rs")
    lock = HERE / "Cargo.lock"
    if lock.exists():
        shutil.copyfile(lock, probe / "Cargo.lock")
    build_args = ["cargo", "+1.95.0", "build", "--release", "--manifest-path", str(probe / "Cargo.toml"), "-j", "4"]
    if lock.exists():
        build_args.append("--locked")
    start = time.perf_counter()
    # Keep all compiler output for diagnosis, and preserve the actual exit code.
    with (root / "comparison-build.log").open("w", encoding="utf-8") as log:
        subprocess.run(build_args, stdout=log, stderr=subprocess.STDOUT, check=True)
    build_s = time.perf_counter() - start
    binary = probe / "target/release" / ("rates-workbench-oss-probe.exe" if os.name == "nt" else "rates-workbench-oss-probe")
    runs = []
    for threads in (1, 4):
        rust = json.loads(command([str(binary), str(threads)]))
        fixture = rust.pop("fixture_cashflows")
        references = rust.pop("reference_pvs_first_41")
        for row in rust["batches"]:
            expected_total = sum(references[i % 41] for i in range(row["instruments"]))
            for name, measurement in row.items():
                if isinstance(measurement, dict) and "checksum" in measurement:
                    np.testing.assert_allclose(measurement["checksum"], expected_total,
                                               rtol=1e-12, atol=1e-9, err_msg=name)
        python = python_comparison(fixture, references, threads)
        runs.append({"threads": threads, "rust": rust, "python": python})
    summary = {
        "date": "2026-09-28", "platform": platform.platform(), "processor": platform.processor(),
        "python": platform.python_version(), "numpy": np.__version__, "numba": numba.__version__,
        "rustc": command(["rustup", "run", "1.95.0", "rustc", "--version"]),
        "pins": {name: {"repository": url, "revision": rev} for name, (url, rev) in PINS.items()},
        "cargo_lock_sha256": hashlib.sha256((probe / "Cargo.lock").read_bytes()).hexdigest(),
        "harness_sha256": {name: hashlib.sha256((HERE / name).read_bytes()).hexdigest()
                           for name in ("main.rs", "compare.py", "Cargo.toml.in")},
        "build_seconds_this_run": build_s,
        "methodology": {
            "instrument": "30-year semiannual fixed bond, face 100, 41 coupons from 2% to 6%",
            "valuation_date": "2026-01-15", "curve": "4% flat continuous plus 100bp constant spread",
            "reference": "sum(amount * exp(-0.05 * actual_days / 365)) on Convex-generated future cashflows",
            "samples": 11, "warmups": 3, "timing": "in-process warm calls; microseconds; no FFI or serialization",
            "parallel": "explicit Rayon adapters around scalar OSS functions; fixed 1/4 threads",
            "excluded": "compilation, input construction, cashflow conversion, dependency graph, service transport",
            "batch_output": "all timings materialize instrument PV vectors and aggregate a checksum",
            "callable_timing": "stochastic-rs reuses a built tree; Convex builds a curve-fitted tree in each call; not comparable models/timings",
            "convex_product_exception": "product pricing regenerates cashflows inside the timed call",
            "python_existing": "base discounting is precomputed in A, as required by corp_pv; OAS exp is timed",
            "custom_controls": "custom fused Rust/Numba loops are controls, not OSS implementations",
            "memory": "Python prepared input bytes only; not process peak memory",
        },
        "runs": runs,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(f"Wrote {args.output}")
    for run in runs:
        print(f"threads={run['threads']}")
        for rust, py in zip(run["rust"]["batches"], run["python"]):
            print(f"  n={rust['instruments']}: current={py['existing_corp_pv_prepared_A']['median_us']:.1f}us, "
                  f"stochastic={rust['stochastic_prepared_parallel']['median_us']:.1f}us, "
                  f"finstack={rust['finstack_prepared_parallel']['median_us']:.1f}us")


if __name__ == "__main__":
    main()
