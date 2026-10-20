#!/usr/bin/env python3
"""Repeat shell commands and report successful wall-clock timings."""

import argparse
import json
import os
import re
import subprocess
import time


def median(xs):
    """Return the median without changing the input, or None when empty."""
    values = sorted(xs)
    size = len(values)
    if not size:
        return None
    middle = size // 2
    if size % 2:
        return values[middle]
    return (values[middle - 1] + values[middle]) / 2


def run_timed(cmd, n=3, warmup=0, env=None, timeout=None):
    """Time n attempts after warmup; warmup outcomes are not counted."""
    if n < 0 or warmup < 0:
        raise ValueError("n and warmup must be non-negative")
    run_env = os.environ.copy()
    if env is not None:
        run_env.update(env)
    times = []
    fails = 0
    for index in range(warmup + n):
        start = time.perf_counter()
        try:
            completed = subprocess.run(
                cmd,
                shell=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                env=run_env,
                timeout=timeout,
            )
            successful = completed.returncode == 0
        except subprocess.TimeoutExpired:
            successful = False
        elapsed = time.perf_counter() - start
        if index < warmup:
            continue
        if successful:
            times.append(float(elapsed))
        else:
            fails += 1
    stats = {
        "min": min(times) if times else None,
        "median": median(times),
        "mean": sum(times) / len(times) if times else None,
        "max": max(times) if times else None,
    }
    return {
        "cmd": cmd,
        "n": n,
        "times": times,
        "fails": fails,
        **{key: round(value, 3) if value is not None else None
           for key, value in stats.items()},
    }


def format_md(rows, unit="s"):
    """Format a table, optionally converting seconds to ms, us, or ns."""
    scale = {"s": 1, "ms": 1_000, "us": 1_000_000, "ns": 1_000_000_000}[unit]
    lines = [
        "| name | runs | min | median | mean | max | fails |",
        "| --- | --- | --- | --- | --- | --- | --- |",
    ]
    for name, result in rows:
        name = str(name).replace("|", r"\|").replace("\r", " ").replace("\n", " ")
        numbers = [
            "n/a" if result[key] is None else f"{result[key] * scale:.3f}"
            for key in ("min", "median", "mean", "max")
        ]
        cells = [name, str(len(result["times"])), *numbers, str(result["fails"])]
        lines.append("| " + " | ".join(cells) + " |")
    return "\n".join(lines)


def format_json(rows):
    """Return the named results as deterministic, indented JSON."""
    return json.dumps(dict(rows), indent=2, sort_keys=True)


def parse_spec(s):
    """Split an optional identifier-like name from a shell command."""
    name, separator, cmd = s.partition("=")
    if separator and re.fullmatch(r"[A-Za-z0-9_.-]+", name):
        return name, cmd
    return s[:40], s


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("-n", type=int, default=3)
    parser.add_argument("--warmup", type=int, default=0)
    parser.add_argument("--timeout", type=float)
    parser.add_argument("--json", action="store_true")
    parser.add_argument("-e", action="append", default=[], metavar="KEY=VAL")
    parser.add_argument("commands", nargs="+", metavar="CMD")
    args = parser.parse_args(argv)
    if args.n < 0 or args.warmup < 0:
        parser.error("n and warmup must be non-negative")
    env = {}
    for assignment in args.e:
        key, separator, value = assignment.partition("=")
        if not separator or not key or "\0" in assignment:
            parser.error("-e requires KEY=VAL with a non-empty key and no NUL")
        env[key] = value
    rows = []
    for spec in args.commands:
        name, cmd = parse_spec(spec)
        rows.append((name, run_timed(
            cmd, n=args.n, warmup=args.warmup, env=env, timeout=args.timeout
        )))
    print(format_json(rows) if args.json else format_md(rows))
    return 1 if any(result["fails"] for _, result in rows) else 0


if __name__ == "__main__":
    raise SystemExit(main())
