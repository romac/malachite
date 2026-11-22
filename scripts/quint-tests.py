#!/usr/bin/env python3
"""Run the standalone Quint suites and compare native evaluator failures by run and source site."""

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPECS = ROOT / "specs"
BASELINE = SPECS / "quint-rust-known-failures.tsv"
# Quint 0.34's JSON report lists failed runs. Their error codes and source sites
# appear in the human-readable diagnostics on a replay with the same seed.
DIAGNOSTIC = re.compile(
    r"^  \d+\) ([\w:]+):\n +Error \[(QNT\d+)\]:[^\n]*\n +at (.+\.qnt):(\d+):(\d+)$",
    re.MULTILINE,
)


def suite_results(spec, args, output):
    cmd = ["quint", "test", str(spec), "--backend", args.backend,
           "--seed", args.seed, "--max-samples", str(args.max_samples)]
    output.unlink(missing_ok=True)
    result = subprocess.run([*cmd, "--out", str(output)], cwd=ROOT)
    report = json.loads(output.read_text())
    passed, failed = report["passed"], report["failed"]
    if (report["stage"] != "testing" or report["errors"] or report["ignored"]
            or not (passed or failed) or result.returncode != int(bool(failed))):
        raise ValueError(f"{spec.name}: incomplete test run, exit {result.returncode}, "
                         f"errors {report['errors']}, ignored {report['ignored']}")
    suite = spec.relative_to(SPECS).as_posix()
    print(f"{suite}: {len(passed)} passed, {len(failed)} failed", flush=True)
    if not failed:
        return len(passed), set()

    replay = subprocess.run(
        cmd, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        env={**os.environ, "FORCE_COLOR": "0"}, text=True,
    )
    sites = {
        (suite, run, code, f"{Path(src).relative_to(SPECS).as_posix()}:{line}:{col}")
        for run, code, src, line, col in DIAGNOSTIC.findall(replay.stdout)
    }
    if replay.returncode != 1 or {row[1] for row in sites} != set(failed) or len(sites) != len(failed):
        raise ValueError(f"Could not reproduce all failure sites in {suite}\n{replay.stdout}")
    return len(passed), sites


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--backend", choices=["rust", "typescript"], default="rust")
    parser.add_argument("--seed", default="118")
    parser.add_argument("--max-samples", type=int, default=100)
    parser.add_argument("--strict", action="store_true", help="require every run to pass")
    args = parser.parse_args()
    if args.max_samples < 1:
        parser.error("--max-samples must be positive")
    expected = set()
    if args.backend == "rust" and not args.strict:
        expected = {tuple(line.split("\t")) for line in BASELINE.read_text().splitlines()
                    if line and not line.startswith("#")}
    observed = set()
    passed = 0
    suites = sorted(SPECS.rglob("*Test.qnt"))
    if not suites:
        raise ValueError("No Quint test suites found")
    with tempfile.TemporaryDirectory() as temp:
        for suite in suites:
            count, sites = suite_results(suite, args, Path(temp) / "result.json")
            passed += count
            observed.update(sites)
    for label, rows in [("Recorded", observed & expected), ("Unexpected", observed - expected),
                        ("No longer observed", expected - observed)]:
        for row in sorted(rows):
            print(f"{label}: {' '.join(row)}")
    print(f"{args.backend}, seed {args.seed}: {len(suites)} suites, {passed} passed, {len(observed)} failed")
    return int(observed != expected)


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, KeyError, ValueError) as error:
        sys.exit(str(error))
