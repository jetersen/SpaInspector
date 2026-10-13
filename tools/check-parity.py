#!/usr/bin/env python3
"""Compare independently parsed Rust/C# exports, including preserved raw bytes."""
import argparse
import csv
import io
import json
import math
import os
from pathlib import Path
import struct
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
FLOAT32_FIELDS = {
    "storedFirstX", "storedLastX", "firstX", "lastX", "signalStrength", "gain",
    "opticalVelocity", "laserFrequencyCmInverse", "backgroundGain", "sampleSpacing",
    "ramanLaserFrequencyCmInverse",
}


def run(command, env=None):
    result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, env=env)
    if result.returncode:
        raise RuntimeError(f"{command[0]} failed ({result.returncode}):\n{result.stdout}{result.stderr}")


def float32(value):
    value = float(value)
    return "NaN" if math.isnan(value) else struct.pack("<f", value)


def compare(left, right, path="document", mode=None):
    if mode == "f32":
        assert float32(left) == float32(right), f"{path}: float32 values differ: {left!r}, {right!r}"
    elif mode == "f64":
        assert math.isclose(float(left), float(right), rel_tol=2e-15, abs_tol=1e-12), f"{path}: coordinates differ"
    elif isinstance(left, dict):
        assert isinstance(right, dict) and left.keys() == right.keys(), f"{path}: schema keys differ"
        for key in left:
            if key == "warnings":  # Wording is implementation-specific, rather than part of numerical parity.
                continue
            child_mode = "f32" if key in FLOAT32_FIELDS and left[key] is not None else None
            if key == "value" and left.get("type") == "float32":
                child_mode = "f32"
            compare(left[key], right[key], f"{path}.{key}", child_mode)
    elif isinstance(left, list):
        assert isinstance(right, list) and len(left) == len(right), f"{path}: array lengths differ"
        child_mode = "f32" if path.endswith(".y") else "f64" if path.endswith(".x") else None
        for index, (a, b) in enumerate(zip(left, right)):
            compare(a, b, f"{path}[{index}]", child_mode)
    else:
        assert left == right, f"{path}: {left!r} != {right!r}"


def compare_csv(left, right):
    a, b = csv.DictReader(io.StringIO(left)), csv.DictReader(io.StringIO(right))
    assert a.fieldnames == b.fieldnames, "CSV schema differs"
    count = 0
    for row_a, row_b in zip(a, b, strict=True):
        for field in a.fieldnames:
            compare(row_a[field], row_b[field], f"CSV row {count} {field}",
                    "f32" if field == "y" else "f64" if field == "x" else None)
        count += 1
    return count


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--corpus", type=Path, default=os.environ.get("SPA_CORPUS_DIR"))
    parser.add_argument("--no-build", action="store_true", help="Use existing Release Rust CLI and C# reader-test builds")
    args = parser.parse_args()
    files = sorted((ROOT / "tests/TestFiles").glob("*.spa"))
    if args.corpus:
        corpus = sorted(path for path in args.corpus.rglob("*") if path.is_file() and path.suffix.lower() == ".spa")
        if not corpus:
            raise ValueError("Requested corpus contains no SPA files")
        files.extend(corpus)
    if not args.no_build:
        run(["cargo", "build", "--locked", "--release", "-p", "spa-reader", "--bin", "spa-convert"])
        run(["dotnet", "build", "tests/SpaInspectorReader.Tests", "-c", "Release", "--nologo"])
    rust = ROOT / "target/release" / ("spa-convert.exe" if os.name == "nt" else "spa-convert")
    rows = objects = 0
    with tempfile.TemporaryDirectory(prefix="spa-parity-") as folder:
        folder = Path(folder)
        manifest = [{"input":str(source.resolve()),
                     "json":str(folder/f"{index}-C#.json"),
                     "csv":str(folder/f"{index}-C#.csv")} for index,source in enumerate(files)]
        manifest_path = folder / "exports.json"
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        environment = os.environ.copy()
        environment["SPA_PARITY_MANIFEST"] = str(manifest_path)
        run(["dotnet", "test", "tests/SpaInspectorReader.Tests", "-c", "Release", "--no-build", "--nologo",
             "--filter", "FullyQualifiedName=SpaInspectorReader.Tests.ParityExportTests.ExportConfiguredFixturesForRustComparison"], env=environment)
        for index, source in enumerate(files):
            exports = {}
            for extension in ["json", "csv"]:
                output = folder / f"{index}-Rust.{extension}"
                run([str(rust), str(source.resolve()), str(output)])
                exports["Rust", extension] = output.read_text(encoding="utf-8")
                exports["C#", extension] = (folder/f"{index}-C#.{extension}").read_text(encoding="utf-8")
            a = json.loads(exports["Rust", "json"])
            b = json.loads(exports["C#", "json"])
            try:
                compare(a, b)
                rows += compare_csv(exports["Rust", "csv"], exports["C#", "csv"])
            except AssertionError as error:
                raise AssertionError(f"{source}: {error}") from error
            objects += len(a["embeddedObjects"])
    print(f"Equivalent JSON/CSV: {len(files)} files, {rows} series points, {objects} embedded objects")


if __name__ == "__main__":
    main()
