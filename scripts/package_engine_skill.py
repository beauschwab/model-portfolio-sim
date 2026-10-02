"""Package the embedded operator skill as a deterministic .skill ZIP.

The installed skill-creator provides validation but no package_skill script.
This uses the same directory-in-ZIP artifact layout without external tools.
"""
from pathlib import Path
from zipfile import ZipFile, ZipInfo, ZIP_DEFLATED

root = Path(__file__).resolve().parents[1] / "packages/portfolio-risk"
source = root / "skills/portfolio-risk-engine"
entry = (source / "SKILL.md").read_text(encoding="utf-8")
assert entry.startswith("---\nname: portfolio-risk-engine\n")
assert "description:" in entry.split("---", 2)[1]
output = root / "dist/portfolio-risk-engine.skill"
output.parent.mkdir(exist_ok=True)
with ZipFile(output, "w", compression=ZIP_DEFLATED) as archive:
    for path in sorted(source.rglob("*.md")):
        item = ZipInfo(str(path.relative_to(source.parent)).replace("\\", "/"))
        item.compress_type = ZIP_DEFLATED
        archive.writestr(item, path.read_bytes())
with ZipFile(output) as archive:
    assert archive.testzip() is None
    assert "portfolio-risk-engine/SKILL.md" in archive.namelist()
print(output)
