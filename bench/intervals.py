#!/usr/bin/env python3
"""Usage: intervals.py LOG [LOG...]  (logs from record_writes)
Per log: counts of kinds, and min/median/p95/max of the gaps in ms between
consecutive write events (create/rename-atomic/append/rewrite) on the same file.
Gaps are computed between the t_ms of observed events; p95 is nearest-rank."""
import sys, math, collections, statistics
for log in sys.argv[1:]:
    ev = collections.defaultdict(list); kinds = collections.Counter(); temps = []
    for l in open(log):
        if l.startswith("#"): continue
        t, k, p, *rest = l.rstrip("\n").split("\t")
        if k == "temp": temps.append(float(rest[1].split("=")[1])); continue
        kinds[k] += 1
        if k != "delete": ev[p].append(float(t))
    gaps = []
    for p, ts in ev.items():
        ts.sort(); gaps += [b - a for a, b in zip(ts, ts[1:])]
    print(log, dict(kinds))
    if gaps:
        gaps.sort(); p95 = gaps[max(0, math.ceil(0.95 * len(gaps)) - 1)]
        print(f"  gaps ms n={len(gaps)} min={gaps[0]:.0f} median={statistics.median(gaps):.0f} p95={p95:.0f} max={gaps[-1]:.0f}")
    if temps:
        print(f"  temp files seen={len(temps)} life ms min={min(temps):.1f} max={max(temps):.1f}")
