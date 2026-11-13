#!/usr/bin/env python3
"""Polling recorder: logs every observed change to *.md under DIR.
Usage: record_writes.py DIR SECONDS OUTFILE
Line: t_ms<TAB>kind<TAB>path<TAB>inode<TAB>size<TAB>mtime_ns
kinds: create delete rename-atomic append rewrite temp
Stops after SECONDS, or on SIGTERM/SIGINT."""
import os, sys, time, signal

POLL = 0.002
root, secs, out = sys.argv[1], float(sys.argv[2]), sys.argv[3]
stop = False
def _s(*_):
    global stop
    stop = True
signal.signal(signal.SIGTERM, _s)
signal.signal(signal.SIGINT, _s)

def scan():
    res = {}
    for d, dirs, files in os.walk(root):
        if ".git" in dirs:
            dirs.remove(".git")
        for f in files:
            p = os.path.join(d, f)
            try:
                st = os.stat(p)
            except OSError:
                continue
            res[p] = (st.st_ino, st.st_size, st.st_mtime_ns)
    return res

state = {}    # md path -> (ino, size, mtime, content)
others = {}   # non-md file -> first seen ms (candidate temp)
t0 = time.monotonic()
ms = lambda: (time.monotonic() - t0) * 1000.0
fo = open(out, "w")
fo.write("# t_ms\tkind\tpath\tinode\tsize\tmtime_ns\n")
def log(kind, p, ino, size, mt):
    fo.write(f"{ms():.1f}\t{kind}\t{os.path.relpath(p, root)}\t{ino}\t{size}\t{mt}\n"); fo.flush()

def read(p):
    try:
        with open(p, "rb") as f:
            return f.read()
    except OSError:
        return None

end = t0 + secs
while not stop and time.monotonic() < end:
    cur = scan()
    for p, (ino, size, mt) in cur.items():
        if p.endswith(".md"):
            old = state.get(p)
            if old and old[:3] == (ino, size, mt):
                continue
            c = read(p)
            if c is None:
                continue
            if old is None:
                kind = "create"
            elif old[0] != ino:
                kind = "rename-atomic"
            elif c.startswith(old[3]) and len(c) > len(old[3]):
                kind = "append"
            else:
                kind = "rewrite"
            state[p] = (ino, size, mt, c)
            log(kind, p, ino, size, mt)
        elif p not in others:
            others[p] = (ms(), ino)
    for p in list(state):
        if p not in cur:
            log("delete", p, state[p][0], 0, 0)
            del state[p]
    for p in list(others):
        if p not in cur:
            t, ino = others.pop(p)
            fo.write(f"{t:.1f}\ttemp\t{os.path.relpath(p, root)}\t{ino}\tlife_ms={ms()-t:.1f}\t-\n"); fo.flush()
    time.sleep(POLL)
fo.close()
