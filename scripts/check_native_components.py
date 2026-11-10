"""Run every native component's Rust gates and retain exact-source evidence.

uv run --no-project --with libclang --with cmake python scripts/check_native_components.py
This does not install or replace the release artifacts used by API/engine tests.
"""
from pathlib import Path
import hashlib
import json
import os
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
CRATES = (
    "portfolio-model-core", "portfolio-compute-control", "portfolio-ledger-native",
    "portfolio-risk-native", "portfolio-decision-native", "portfolio-verification-native",
)


def source_hashes():
    names = subprocess.check_output(
        ["git", "ls-files", "-z", "--", *(f"packages/{name}" for name in CRATES)],
        cwd=ROOT,
    ).decode().split("\0")
    return {
        name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
        for name in sorted(names) if name
    }


def main():
    import clang.cindex
    import cmake

    out = ROOT / "artifacts/native-components"
    out.mkdir(parents=True, exist_ok=False)
    env = os.environ.copy()
    env.update(
        CARGO_TARGET_DIR=str(ROOT / "target/native-components"),
        CARGO_INCREMENTAL="0", CARGO_PROFILE_DEV_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0",
        CARGO_BUILD_JOBS="2", LIBCLANG_PATH=clang.cindex.Config.library_path,
        CMAKE=str(Path(cmake.CMAKE_BIN_DIR) / ("cmake.exe" if os.name == "nt" else "cmake")),
    )
    before = source_hashes()
    receipt = {
        "head_sha": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "source_sha256": before, "gates": [], "all_passed": False,
        "release_artifacts_installed": False,
    }
    try:
        for crate in CRATES:
            manifest = str(ROOT / "packages" / crate / "Cargo.toml")
            gates = (
                ("format", ["cargo", "fmt", "--manifest-path", manifest, "--", "--check"]),
                ("tests", ["cargo", "test", "--locked", "--no-fail-fast", "--all-targets", "--manifest-path", manifest,
                           "--", "--test-threads=2"]),
                ("strict", ["cargo", "clippy", "--locked", "--all-targets", "--all-features",
                            "--manifest-path", manifest, "--", "-D", "warnings"]),
                ("docs", ["cargo", "test", "--locked", "--doc", "--manifest-path", manifest]),
            )
            for name, args in gates:
                started = time.monotonic()
                log = out / f"{crate}-{name}.log"
                print(f"START {crate} {name}", flush=True)
                with log.open("wb") as stream:
                    result = subprocess.run(args, cwd=ROOT, env=env, stdout=stream, stderr=subprocess.STDOUT)
                receipt["gates"].append({
                    "crate": crate, "gate": name, "command": args, "exit_code": result.returncode,
                    "seconds": time.monotonic() - started,
                    "log_sha256": hashlib.sha256(log.read_bytes()).hexdigest(),
                })
                print(log.read_text(encoding="utf8", errors="replace"), flush=True)
                if source_hashes() != before:
                    raise RuntimeError("native component source changed during validation")
        receipt["all_passed"] = all(gate["exit_code"] == 0 for gate in receipt["gates"])
    finally:
        receipt["source_unchanged"] = source_hashes() == before
        if not receipt["source_unchanged"]:
            receipt["all_passed"] = False
        (out / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf8")
    if not receipt["all_passed"]:
        raise RuntimeError("native component validation incomplete")


if __name__ == "__main__":
    main()
