#!/usr/bin/env python3
"""Summarise differential-suite logs as a GitHub markdown table."""

import argparse
from pathlib import Path
import re


_SECONDS = re.compile(r"(?<!\S)(\d+(?:\.\d*)?|\.\d+)(?:[eE]([+-]?\d+))?s(?!\S)")
_DIFFERENT = re.compile(r"(?<!\S)(\d+)\s+DIFFERENT\b")
_HEADER = "| suite | files | ok | failed | DIFFERENT | oracle s | fast s | speedup |"
_SEPARATOR = "| --- | --- | --- | --- | --- | --- | --- | --- |"


def _seconds(text):
    values = [
        float(match[1] + ("e" + match[2] if match[2] else ""))
        for match in _SECONDS.finditer(text)
    ]
    return sum(values) if values else None


def parse_line(line):
    """Parse one result, returning None for blank lines and log headers."""
    line = line.strip()
    if not line or line.startswith(("#", "==")):
        return None
    tokens = line.split()
    if "FAILED" in line:
        status = "failed"
    elif tokens[-1] == "DIFFERENT" or any(
        int(match[1]) > 0 for match in _DIFFERENT.finditer(line)
    ):
        status = "different"
    else:
        status = "ok"

    oracle_s = fast_s = None
    oracle = re.search(r"\boracle\b", line)
    if oracle:
        remainder = line[oracle.end():]
        fast = re.search(r"\bfast\b", remainder)
        if fast:
            oracle_s = _seconds(remainder[:fast.start()].split("|", 1)[0])
            fast_s = _seconds(remainder[fast.end():])
    if oracle_s is None or fast_s is None:
        oracle_s = fast_s = None
    return {
        "name": tokens[0],
        "status": status,
        "oracle_s": oracle_s,
        "fast_s": fast_s,
    }


def summarise(lines):
    """Count the last result for each file and sum complete timing pairs."""
    latest = {}
    for line in lines:
        result = parse_line(line)
        if result is not None:
            latest[result["name"]] = result
    summary = {
        "files": len(latest),
        "ok": 0,
        "failed": 0,
        "different": 0,
        "oracle_s": 0.0,
        "fast_s": 0.0,
    }
    for result in latest.values():
        summary[result["status"]] += 1
        if result["oracle_s"] is not None and result["fast_s"] is not None:
            summary["oracle_s"] += result["oracle_s"]
            summary["fast_s"] += result["fast_s"]
    summary["speedup"] = (
        round(summary["oracle_s"] / summary["fast_s"], 1)
        if summary["fast_s"] else None
    )
    return summary


def suite_name(path, prefix=""):
    """Remove the conventional filename wrapper and optional suite prefix."""
    name = Path(path).name
    if name.startswith("h1_"):
        name = name[3:]
    if prefix and name.startswith(prefix):
        name = name[len(prefix):]
    if name.endswith(".log"):
        name = name[:-4]
    return name


def _read_suites(paths, prefix):
    for path in paths:
        path = Path(path)
        if str(path).endswith(".err"):
            continue
        try:
            with path.open(encoding="utf-8") as stream:
                summary = summarise(stream)
        except (OSError, UnicodeError):
            summary = None
        yield suite_name(path, prefix), summary


def _table(suites):
    rows = [_HEADER, _SEPARATOR]
    for name, summary in suites:
        name = name.replace("|", r"\|").replace("\n", " ").replace("\r", " ")
        if summary is None:
            cells = ["n/a"] * 7
        else:
            cells = [str(summary[key]) for key in ("files", "ok", "failed", "different")]
            cells.extend(f"{summary[key]:.1f}" for key in ("oracle_s", "fast_s"))
            speedup = summary["speedup"]
            cells.append(f"{speedup:.1f}x" if speedup is not None else "n/a")
        rows.append("| " + " | ".join([name] + cells) + " |")
    return "\n".join(rows)


def report(paths, prefix=""):
    """Render paths in input order, retaining unreadable suites as n/a rows."""
    return _table(_read_suites(paths, prefix))


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", default="")
    parser.add_argument("paths", nargs="+", metavar="PATH_OR_DIR")
    args = parser.parse_args(argv)
    paths = []
    for argument in args.paths:
        path = Path(argument)
        if path.is_dir():
            paths.extend(sorted(
                (entry for entry in path.iterdir()
                 if entry.is_file()
                 and entry.name.startswith("h1_" + args.prefix)
                 and entry.name.endswith(".log")
                 and (args.prefix or entry.name != "h1_summary.log")),
                key=lambda entry: entry.name,
            ))
        else:
            paths.append(path)
    suites = list(_read_suites(paths, args.prefix))
    print(_table(suites))
    return int(any(
        summary and (summary["failed"] or summary["different"])
        for _, summary in suites
    ))


if __name__ == "__main__":
    raise SystemExit(main())
