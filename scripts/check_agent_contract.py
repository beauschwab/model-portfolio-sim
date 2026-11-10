"""Keep the root agent contract small enough to load on every task."""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MAX_LINES = 150


def violations(root: Path) -> list[str]:
    path = root / "AGENTS.md"
    if not path.is_file():
        return ["AGENTS.md: current agent contract is missing"]
    count = len(path.read_text(encoding="utf-8").splitlines())
    if count > MAX_LINES:
        return [
            f"AGENTS.md: {count} lines exceeds the {MAX_LINES}-line limit; "
            "keep current rules here and move dated history to docs/changelog/agents-history.md"
        ]
    return []


def main() -> None:
    errors = violations(ROOT)
    if errors:
        raise SystemExit("\n".join(errors))
    print(f"Root agent contract verified (at most {MAX_LINES} lines)")


if __name__ == "__main__":
    main()
