#!/usr/bin/env python3
"""Reject private numerical processing material in the public release tree."""
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FORBIDDEN_PARTS = {
    "advanced-atr", "interferogram", "atr-benchmark",
    "advanced_atr.rs", "interferogram.rs", "atr.rs", "AdvancedAtr.cs",
    "InterferogramReprocessing.cs", "AdvancedAtrTests.cs",
    "InterferogramReprocessingTests.cs", "advanced-atr.md", "atr-performance.md",
    "interferogram-reprocessing.md", "background-processing.md",
    "atmospheric-suppression.md", "benchmark_atr.rs", "benchmark_atr_wasm.rs",
    "generate-interferogram-fixture.py",
}
files = subprocess.check_output(
    ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=ROOT,
).decode().split("\0")
failures = []
for name in set(files):
    path = ROOT / name
    if not name or not path.is_file():
        continue
    if FORBIDDEN_PARTS.intersection(Path(name).parts):
        failures.append(name)
    if path.name in {"Cargo.toml", "Cargo.lock"} or path.suffix in {".csproj", ".props", ".targets"}:
        text = path.read_text()
        if "SpaInspector-processing" in text or "spa-processing" in text or "SpaInspectorProcessing" in text:
            failures.append(f"{name}: private processing dependency")
if failures:
    raise SystemExit("Private material in public tree:\n" + "\n".join(sorted(failures)))
print("Public boundary check passed")
