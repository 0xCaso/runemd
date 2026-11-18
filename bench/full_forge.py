#!/usr/bin/env python3
"""Full forge cost (docs/perf-memory-2026-11-16.md, "Full forge 2026-11-18"): RUNEMD_FORGE off, each
direction alone, all on; idle CPU and scroll frames of the bundled app on a private 120 Hz virtual
display (`~/.claude/bin/vscreen`, display `forge-agent`, created by the caller).

Idle: fixtures/forge.md, dark, empty RUNEMD_ROOTS, fresh RUNE_DIRS_ROOT, MDSPIKE_ACTIVE=1; the app's
PID only, 10-70 s after launch: cpu_time = `ps -o time` delta / 60 s, ps_mean = 60 samples of
`ps -o %cpu`, frames/s = MDSPIKE_FRAMES lines with drawn=1 in the window.
Scroll: MDSPIKE_SCROLL=4, fixtures/forge.md at 600 pt/s (it is short) and fixtures/big.md at
3000 pt/s; the `scroll frames` line.
usage: bench/full_forge.py WORKDIR RUNS [CONFIG ...]   (CONFIG: off, a direction name, all)
"""
import json, os, subprocess, sys, time

VS = os.path.expanduser("~/.claude/bin/vscreen")
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
APP = os.path.join(ROOT, "build/RuneMD.app")
ALL = ["off", "steel", "forged", "engraved", "ember", "glass", "tasks", "electric", "all"]


def ours():
    """PIDs of this bundle's app (only the ones this script started run from it)."""
    out = subprocess.run(["pgrep", "-f", f"{APP}/Contents/MacOS/runemd"], capture_output=True, text=True).stdout
    return {int(p) for p in out.split()}


def launch(w, tag, forge, extra):
    extra = dict(extra)
    args = extra.pop("_args", [])
    os.makedirs(f"{w}/empty", exist_ok=True)
    env = {"RUNEMD_THEME": "dark", "RUNEMD_ROOTS": f"{w}/empty", "RUNE_DIRS_ROOT": f"{w}/dirs-{tag}-{time.time_ns()}", "MDSPIKE_ACTIVE": "1", **extra}
    if forge != "off":
        env["RUNEMD_FORGE"] = forge
    cmd = [VS, "launch", "--name", "forge-agent", "--log", f"{w}/{tag}.stderr", "--env", ",".join(f"{k}={v}" for k, v in env.items()), "--", APP, *args]
    for attempt in range(3):
        before = ours()
        r = subprocess.run(cmd, capture_output=True, text=True)
        if r.returncode == 0:
            return json.loads(r.stdout)["pid"]
        # Exit 5: a window is off the virtual display (a 500x500 helper window, now and then); the
        # app's main window is on it, so the run is fine.
        try:
            j = json.loads(r.stdout)
            if any(x["onDisplay"] and x["frame"]["height"] > 300 for x in j["windows"]):
                return j["pid"]
        except (ValueError, KeyError):
            pass
        # `vscreen launch` exits 5 now and then with the app started (its window late under load):
        # adopt that app when its window is on the virtual display, otherwise stop it and try again.
        print(f"launch failed ({r.returncode}): {r.stderr.strip()}", file=sys.stderr, flush=True)
        time.sleep(3)
        for pid in ours() - before:
            win = subprocess.run([VS, "windows", "--pid", str(pid)], capture_output=True, text=True).stdout
            if '"onDisplay" : true' in win:
                print(f"adopted {pid}", file=sys.stderr, flush=True)
                return pid
            os.kill(pid, 15)
        time.sleep(2)
    r.check_returncode()


def cpu_s(pid):
    t = subprocess.run(["ps", "-o", "time=", "-p", str(pid)], capture_output=True, text=True).stdout.strip()
    m, s = t.split(":")
    return int(m) * 60 + float(s)


def idle(w, forge, run):
    tag = f"idle-{forge}-{run}"
    frames = f"{w}/{tag}.frames"
    load = os.getloadavg()[0]
    t0 = time.time()
    pid = launch(w, tag, forge, {"MDSPIKE_FRAMES": frames, "_args": [f"{ROOT}/fixtures/forge.md"]})
    time.sleep(max(0.0, t0 + 10 - time.time()))
    a, ea = cpu_s(pid), time.time() * 1000
    samples = []
    for _ in range(60):
        samples.append(float(subprocess.run(["ps", "-o", "%cpu=", "-p", str(pid)], capture_output=True, text=True).stdout.strip() or 0))
        time.sleep(max(0.0, (ea / 1000 + len(samples)) - time.time()))
    b, eb = cpu_s(pid), time.time() * 1000
    os.kill(pid, 15)
    drawn = []
    with open(frames) as f:
        for line in f:
            try:  # a line cut short by the kill is skipped
                p = dict(x.split("=", 1) for x in line.split()[1:])
                if ea <= float(p["epoch_ms"]) <= eb and p["drawn"] == "1":
                    drawn.append(float(p["update_ms"]) + float(p["encode_ms"]))
            except (KeyError, ValueError):
                continue
    secs = (eb - ea) / 1000
    drawn.sort()
    pct = lambda q: drawn[round((len(drawn) - 1) * q)] if drawn else 0.0
    return {"kind": "idle", "forge": forge, "run": run, "cpu_time": 100 * (b - a) / secs, "ps_mean": sum(samples) / len(samples), "fps": len(drawn) / secs, "frame_p50": pct(0.5), "frame_p95": pct(0.95), "load": load}


def scroll(w, forge, run, fixture, speed):
    tag = f"scroll-{fixture}-{forge}-{run}"
    log = f"{w}/{tag}.log"
    load = os.getloadavg()[0]
    pid = launch(w, tag, forge, {"MDSPIKE_LOG": log, "MDSPIKE_SCROLL": "4", "MDSPIKE_SPEED": str(speed), "_args": [f"{ROOT}/fixtures/{fixture}.md"]})
    for _ in range(300):
        if subprocess.run(["kill", "-0", str(pid)], capture_output=True).returncode != 0:
            break
        time.sleep(0.1)
    else:
        os.kill(pid, 15)
    line = next((l for l in open(log) if l.startswith("scroll frames")), "")
    p = dict(x.split("=", 1) for x in line.split()[1:])
    return {"kind": "scroll", "fixture": fixture, "forge": forge, "run": run, "load": load, **{k: float(v) for k, v in p.items()}}


def main():
    w, runs = sys.argv[1], int(sys.argv[2])
    configs = sys.argv[3:] or ALL
    os.makedirs(w, exist_ok=True)
    done = set()
    if os.path.exists(f"{w}/results.jsonl"):
        for l in open(f"{w}/results.jsonl"):
            d = json.loads(l)
            done.add((d["kind"], d.get("fixture"), d["forge"], d["run"]))
    out = open(f"{w}/results.jsonl", "a")
    for run in range(1, runs + 1):
        for forge in configs:
            jobs = [(("idle", None), lambda: idle(w, forge, run)), (("scroll", "forge"), lambda: scroll(w, forge, run, "forge", 600))]
            if forge in ("off", "all"):
                jobs.append((("scroll", "big"), lambda: scroll(w, forge, run, "big", 3000)))
            for (kind, fixture), job in jobs:
                if (kind, fixture, forge, run) in done:
                    continue
                busy = subprocess.run("pgrep -x cargo; pgrep rustc", shell=True, capture_output=True, text=True).stdout.strip()
                while busy:
                    time.sleep(10)
                    busy = subprocess.run("pgrep -x cargo; pgrep rustc", shell=True, capture_output=True, text=True).stdout.strip()
                r = job()
                print(json.dumps(r), flush=True)
                out.write(json.dumps(r) + "\n"); out.flush()
                time.sleep(1)


main()
