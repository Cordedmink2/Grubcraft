#!/usr/bin/env python3
"""Smoke run of the real game binary, for CI (docs/10-architecture.md, Testing).

    smoke-run.py PATH-TO-GRUBCRAFT [FRAMES]

Starts the game straight into a battle against the computer, draws FRAMES
frames (default 300) on whatever software GPU the machine has, and quits. It
fails if the game panics, exits with an error, logs a lost GPU, takes longer
than fifteen minutes, or ends using more than the 6 GB memory budget. The game's
settings, log and crash reports go in a temporary folder.
"""
import os
import pathlib
import re
import subprocess
import sys
import tempfile

MEMORY_BUDGET_MB = 6144
TIME_LIMIT_S = 900


def main():
    if not 2 <= len(sys.argv) <= 3:
        sys.exit(__doc__)
    binary = pathlib.Path(sys.argv[1]).resolve()
    frames = sys.argv[2] if len(sys.argv) == 3 else "300"
    app_dir = pathlib.Path(tempfile.mkdtemp(prefix="grubcraft-smoke-"))
    env = dict(os.environ, GRUBCRAFT_SETTINGS=str(app_dir / "settings.ron"))
    # Shadows and glow are left out only because a software GPU draws them
    # very slowly; everything else is what a player runs.
    cmd = [
        str(binary),
        "--battle", "recruit",
        "--frames", frames,
        "--no-audio",
        "--no-shadows",
        "--no-glow",
    ]
    try:
        run = subprocess.run(
            cmd, env=env, capture_output=True, text=True, timeout=TIME_LIMIT_S
        )
    except subprocess.TimeoutExpired as e:
        print(e.stdout or "")
        sys.exit(f"FAIL: still running after {TIME_LIMIT_S} s")
    print(run.stdout)
    print(run.stderr, file=sys.stderr)

    problems = []
    if run.returncode != 0:
        problems.append(f"exit code {run.returncode}")
    log = app_dir / "logs" / "grubcraft.log"
    log_text = log.read_text(errors="replace") if log.exists() else ""
    for word in ("PANIC", "GPU device lost"):
        if word in log_text or word in run.stderr:
            problems.append(f"the log says {word!r}")
    crashes = list((app_dir / "crashes").glob("*")) if (app_dir / "crashes").exists() else []
    if crashes:
        problems.append(f"crash report written: {crashes[0].name}")
    memory = re.search(r"([\d.]+) MB process", run.stdout)
    if memory is None:
        problems.append("no memory figure in the game's final line (did it draw its frames?)")
    elif float(memory.group(1)) > MEMORY_BUDGET_MB:
        problems.append(f"{memory.group(1)} MB used, budget {MEMORY_BUDGET_MB} MB")
    if problems:
        print(f"FAIL: {'; '.join(problems)}")
        if log_text:
            print(f"--- {log} ---\n{log_text}")
        sys.exit(1)
    print(f"ok: {frames} frames, {memory.group(1)} MB process (budget {MEMORY_BUDGET_MB} MB)")


main()
