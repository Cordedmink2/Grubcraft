#!/usr/bin/env python3
"""Instruction-count gate for the simulation benchmarks (docs/12-performance.md).

Runs each scenario of gc_bench under valgrind's callgrind and compares the
number of instructions executed with tools/gc_bench/baseline.json. Instruction
counts do not depend on how busy the machine is, so a 5% limit is steady
where a timing limit on a shared CI runner is not. Linux only (valgrind).

    scripts/bench-gate.py          fail if any scenario runs over 5% more instructions
    scripts/bench-gate.py bless    write the current counts as the new baseline

A change that makes the simulation do more work on purpose (or less) re-blesses
the baseline in the same pull request, with the reason in the description.
The counts also go to target/bench/instructions.json.
"""
import json
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
BASELINE = ROOT / "tools/gc_bench/baseline.json"
OUT = ROOT / "target/bench"
BENCH = ROOT / "target/release/gc_bench"
LIMIT = 0.05

# Sized to take about half a minute each under callgrind.
SCENARIOS = {
    "drill": ["--ticks", "1200", "--regiments-per-player", "150"],
    "pathing-storm": [],
    "field-8000pt-2v2": [],
}


def measure(name, args):
    out = OUT / f"{name}.callgrind"
    cmd = [
        "valgrind",
        "--tool=callgrind",
        f"--callgrind-out-file={out}",
        str(BENCH),
        "--scenario",
        name,
        *args,
    ]
    run = subprocess.run(cmd, capture_output=True, text=True)
    # Under valgrind the wall-clock budgets are missed (exit 1); only a crash
    # or a run that never finished matters here.
    if run.returncode not in (0, 1) or "final hash" not in run.stdout:
        sys.exit(f"{name}: gc_bench did not finish\n{run.stdout}\n{run.stderr}")
    totals = re.search(r"^totals:\s+(\d+)", out.read_text(), re.M)
    if not totals:
        sys.exit(f"{name}: no instruction total in {out}")
    return int(totals.group(1))


def main():
    bless = sys.argv[1:] == ["bless"]
    if sys.argv[1:] and not bless:
        sys.exit(__doc__)
    subprocess.run(["cargo", "build", "--release", "-p", "gc_bench"], cwd=ROOT, check=True)
    OUT.mkdir(parents=True, exist_ok=True)
    current = {
        name: {"args": args, "instructions": measure(name, args)}
        for name, args in SCENARIOS.items()
    }
    (OUT / "instructions.json").write_text(json.dumps(current, indent=2) + "\n")
    if bless:
        BASELINE.write_text(json.dumps(current, indent=2) + "\n")
        print(f"wrote {BASELINE.relative_to(ROOT)}")
        return

    baseline = json.loads(BASELINE.read_text())
    failed = False
    for name, now in current.items():
        before = baseline.get(name)
        if before is None or before["args"] != now["args"]:
            print(f"FAIL {name}: not in the baseline as run; run `scripts/bench-gate.py bless`")
            failed = True
            continue
        change = now["instructions"] / before["instructions"] - 1
        verdict = "ok"
        if change > LIMIT:
            verdict = f"FAIL: over the {LIMIT:.0%} limit"
            failed = True
        elif change < -LIMIT:
            verdict = "faster: run `scripts/bench-gate.py bless` to keep the gain"
        print(
            f"{name:18} {now['instructions']:>15,} instructions "
            f"({change:+.2%} against {before['instructions']:,})  {verdict}"
        )
    if failed:
        sys.exit(1)


main()
