#!/usr/bin/env python3
"""Display the reader's JSON progress snapshots in a terminal."""

import argparse
import json
import math
import os
from pathlib import Path
import sys
import time


def bar(done, total, width=10):
    """Return a fixed-width progress bar, clamped to its bounds."""
    filled = max(0, min(width, math.floor(width * done / total))) if total > 0 else 0
    return "[" + "#" * filled + "-" * (width - filled) + "]"


def fmt_bytes(n):
    """Format a byte count using binary multiples."""
    if n is None or n < 0:
        n = 0
    if n < 1024:
        return f"{int(n)} B"
    for unit in ("KB", "MB", "GB"):
        n /= 1024
        if n < 1024 or unit == "GB":
            return f"{n:.1f} {unit}"


def fmt_eta(eta_s):
    """Format an estimated duration in seconds, minutes, or hours."""
    if eta_s is None:
        return "?"
    if eta_s < 60:
        return f"{eta_s:.1f}s"
    seconds = int(eta_s)
    if seconds < 3600:
        return f"{seconds // 60}m{seconds % 60:02d}s"
    return f"{seconds // 3600}h{seconds % 3600 // 60:02d}m"


def render(d, now, width=100):
    """Render one snapshot without consulting the clock or filesystem."""
    name = os.path.basename(d["file"])[:28]
    stage = d.get("stage", "")
    done, total = d.get("done", 0), d.get("total", 0)
    final = d.get("final", False)
    pct = 100 if final else int(100 * done / total) if total > 0 else 0
    eta = "done" if final else fmt_eta(d.get("eta_s"))
    line = (
        f"{name:<28}  {stage:<10}  {bar(done, total)}  {pct}%"
        f"  eta {eta}  {fmt_bytes(d.get('bytes_read'))}"
    )
    age = now - d.get("updated", now)
    if age > 30 and not final:
        line += f"  (stale {int(age)}s)"
    return line[:width]


def load_all(dir):
    """Read valid snapshots, newest first, ignoring incomplete writes."""
    records = []
    try:
        paths = list(Path(dir).glob("*.json"))
    except OSError:
        return records
    for path in paths:
        try:
            with path.open(encoding="utf-8") as source:
                record = json.load(source)
        except (OSError, ValueError, UnicodeError):
            continue
        if isinstance(record, dict) and "file" in record:
            records.append(record)
    return sorted(records, key=lambda record: record.get("updated", 0), reverse=True)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dir", default="~/.cache/rvt-progress")
    parser.add_argument("--once", action="store_true")
    parser.add_argument("--watch", type=float, default=1.0, metavar="SEC")
    parser.add_argument("--active", action="store_true")
    args = parser.parse_args(argv)
    if not math.isfinite(args.watch) or args.watch <= 0:
        parser.error("--watch must be a positive, finite number of seconds")

    once = args.once or not sys.stdout.isatty()
    directory = os.path.expanduser(args.dir)
    try:
        while True:
            records = load_all(directory)
            if args.active:
                records = [record for record in records if not record.get("final", False)]
            now = time.time()
            lines = [render(record, now) for record in records]
            if not once:
                sys.stdout.write("\033[2J\033[H")
            print("\n".join(lines) if lines else "no progress files", flush=True)
            if once:
                return 0
            time.sleep(args.watch)
    except KeyboardInterrupt:
        return 0


if __name__ == "__main__":
    sys.exit(main())
