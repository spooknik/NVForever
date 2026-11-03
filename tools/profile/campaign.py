"""The profiling campaign: every scenario of tools/profile/campaign.toml as one launch of a
`profile` build, and a report that ranks where the time goes (docs/design/frame-rate-audit.md §5).

    python tools/profile/campaign.py run --exe F:/nvf-target/profile/release/nvforever.exe --data <Data> [--only NAME…] [--group G…] [--name RUN]
    python tools/profile/campaign.py report target/profile-campaign/<run>

Build the exe with `cargo build --release -p nvforever --features nvforever/profile` in its own
target folder. Each launch goes through tools/stress's launcher (the game locks, its own
NVForever home without stored settings, hang and crash handling, memory samples), with the
span profile (`NVF_PLAY_TEST_PROFILE`), the frame-time log (vsync off) and the stress samples.
Runs write `target/profile-campaign/<run>/results.json` and `report.md`. The logs come from
game data: keep them out of the repository.

Times are milliseconds a frame. Spans that run on other threads add up beyond the frame time;
the frame split (main schedule, render schedule, the wait for the window's next image) says
which thread holds the frame.
"""
from __future__ import annotations

import argparse
import json
import re
import statistics
import sys
import time
import tomllib
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
sys.path.insert(0, str(REPO / "tools" / "stress"))

FRAME = re.compile(r"frame time: ([\d.]+) ms mean, ([\d.]+) ms slowest over (\d+) frames")
WINDOW = re.compile(r"profile: ([\d.]+) s, (\d+) frames, frame mean ([\d.]+) ms(?:, cpu ([\d.]+) ms a frame)?; per frame ms: (.*)$")
GPU = re.compile(r"profile gpu: ms a frame: (.*)$")
HITCH = re.compile(r"profile hitch( \(behind a loading screen\))?: ([\d.]+) ms; spans ms: (.*?)(?:; pipelines (\d+) \((\d+) new.*)?$")
SCENE = re.compile(r"profile scene: (\d+) mesh entities \((\d+) visible, (\d+) moved this frame\), (\d+) distinct meshes, (\d+) distinct materials")
VISIBILITY = re.compile(r"profile visibility: (\d+) entities with visibility")
TRANSFORMS = re.compile(r"profile transforms: (\d+) of (\d+) written this frame \((\d+) with a mesh")
SHADOWS = re.compile(r"profile shadows: casters per cascade, nearest first: (.*)$")
SAMPLE = re.compile(r"stress sample: t ([\d.]+) s, frames (\d+), frame mean ([\d.]+) ms, frame worst ([\d.]+) ms.*?actors (\d+).*?, at (.*)$")
ITEM = re.compile(r"(.+?) ([\d.]+)(?: \((\d+)\))?$")

# Spans that only hold other spans or a whole schedule: shown in the frame split, left out
# of the heaviest-span lists.
WRAPPERS = {
    "update", "main app", "Main", "RenderApp", "RenderRecovery", "bevy_render::run_render_schedule", "Render",
    "Update", "PostUpdate", "PreUpdate", "First", "Last", "FixedUpdate", "FixedMain", "RunFixedMainLoop",
    "FixedPreUpdate", "FixedPostUpdate", "SpawnScene", "StateTransition", "PreStartup", "Startup", "PostStartup",
    "renderer::render_system", "main_render_schedule", "RenderGraph", "multithreaded executor", "camera_schedule",
    "schedule::camera_driver", "Core3d", "ExtractSchedule", "RenderExtractApp", "par_for_each",
    "finish_command_buffer", "finish_command_buffers", "transform propagation worker", "executor",
}
# The frame split: which thread holds the frame.
SPLIT = ["Main", "Render", "RenderExtractApp", "window::prepare_windows", "queue_submit"]


def stamp(line: str) -> float | None:
    """Seconds of the day from the log line's time stamp."""
    m = re.search(r"T(\d\d):(\d\d):(\d\d(?:\.\d+)?)Z", line)
    return int(m.group(1)) * 3600 + int(m.group(2)) * 60 + float(m.group(3)) if m else None


def items(text: str) -> dict[str, float]:
    """`name 1.23, other 0.4 (12), …` as {name: ms}. Names may hold commas inside <…>."""
    out: dict[str, float] = {}
    depth, cur = 0, ""
    parts = []
    for ch in text:
        depth += ch == "<"
        depth -= ch == ">"
        if ch == "," and depth == 0:
            parts.append(cur)
            cur = ""
        else:
            cur += ch
    parts.append(cur)
    for p in parts:
        if m := ITEM.match(p.strip()):
            out[m.group(1)] = out.get(m.group(1), 0.0) + float(m.group(2))
    return out


def mean_dicts(ds: list[dict[str, float]]) -> dict[str, float]:
    if not ds:
        return {}
    keys = set().union(*ds)
    return {k: round(sum(d.get(k, 0.0) for d in ds) / len(ds), 3) for k in keys}


def top(d: dict[str, float], n: int, skip=frozenset()) -> list[tuple[str, float]]:
    return sorted(((k, v) for k, v in d.items() if k not in skip), key=lambda kv: -kv[1])[:n]


def parse(lines: list[str], warmup: float) -> dict:
    """What one launch's log says, its first `warmup` seconds after the first frame-time line
    left out (loading, pipelines, spawning)."""
    start = None
    for ln in lines:
        if FRAME.search(ln):
            start = stamp(ln)
            break
    def since(t):
        # Seconds after the first frame-time line; a run that passes midnight wraps once.
        d = t - start
        return d + 86400 if d < -43200 else d

    def counted(ln):
        t = stamp(ln)
        return start is not None and t is not None and since(t) >= warmup

    frames, windows, gpus, hitches, loading_hitches, scenes, samples = [], [], [], [], [], [], []
    vis, transforms, shadows = [], [], ""
    slow_spans: list[dict[str, float]] = []
    for ln in lines:
        if m := HITCH.search(ln):
            if not counted(ln):
                continue
            all_spans = items(m.group(3))
            h = {"ms": float(m.group(2)), "spans": top(all_spans, 8, WRAPPERS), "t": stamp(ln),
                 "new_pipelines": int(m.group(5)) if m.group(5) else None}
            if m.group(1):
                loading_hitches.append(h)
            else:
                hitches.append(h)
                slow_spans.append(all_spans)
            continue
        if m := SAMPLE.search(ln):
            samples.append({"t": float(m.group(1)), "mean": float(m.group(3)), "worst": float(m.group(4)),
                            "actors": int(m.group(5)), "at": m.group(6).strip()})
        if not counted(ln):
            continue
        if m := FRAME.search(ln):
            frames.append((float(m.group(1)), float(m.group(2)), int(m.group(3))))
        elif m := WINDOW.search(ln):
            spans = items(m.group(5))
            windows.append({"mean": float(m.group(3)), "cpu": float(m.group(4)) if m.group(4) else None, "spans": spans})
        elif m := GPU.search(ln):
            gpus.append(items(m.group(1)))
        elif m := SCENE.search(ln):
            scenes.append(tuple(int(m.group(i)) for i in range(1, 6)))
        elif m := VISIBILITY.search(ln):
            vis.append(int(m.group(1)))
        elif m := TRANSFORMS.search(ln):
            transforms.append((int(m.group(1)), int(m.group(2)), int(m.group(3))))
        elif m := SHADOWS.search(ln):
            shadows = m.group(1)
    spans = mean_dicts([w["spans"] for w in windows])
    gpu = mean_dicts(gpus)
    means = [f[0] for f in frames]
    cpus = [w["cpu"] for w in windows if w["cpu"] is not None]
    hitch_start = start or 0.0
    # What grows in the slow frames: each span's mean over them against its mean over every frame.
    slow = mean_dicts(slow_spans)
    excess = {k: round(v - spans.get(k, 0.0), 2) for k, v in slow.items() if k not in WRAPPERS}
    for h in hitches + loading_hitches:
        h["t"] = round(since(h["t"]), 1) if h["t"] is not None and start is not None else None
    return {
        "periods": len(frames),
        "frame_mean": round(statistics.median(means), 2) if means else None,
        "frame_worst_period": round(max(means), 2) if means else None,
        "slowest_frame": round(max(f[1] for f in frames), 1) if frames else None,
        "cpu_ms": round(statistics.median(cpus), 1) if cpus else None,
        "split": {k: spans.get(k) for k in SPLIT},
        "spans": dict(top(spans, 40, WRAPPERS)),
        "gpu": dict(top(gpu, 20)),
        "gpu_total": round(sum(gpu.values()), 2) if gpu else None,
        "hitches": sorted(hitches, key=lambda h: -h["ms"]),
        "slow_frame_excess": dict(top(excess, 8)),
        "loading_hitches": len(loading_hitches),
        "scene": {"meshes": scenes[-1][0], "visible": scenes[-1][1], "moved": scenes[-1][2], "distinct_meshes": scenes[-1][3],
                  "materials": scenes[-1][4]} if scenes else None,
        "entities_with_visibility": vis[-1] if vis else None,
        "transforms": {"written": transforms[-1][0], "of": transforms[-1][1], "with_mesh": transforms[-1][2]} if transforms else None,
        "shadow_casters": shadows,
        "actors": max((s["actors"] for s in samples), default=None),
        "first_at": samples[0]["at"] if samples else None,
        "last_at": samples[-1]["at"] if samples else None,
    }


# ---------------------------------------------------------------- report

def fmt(v, unit="") -> str:
    return "" if v is None else f"{v:g}{unit}" if isinstance(v, (int, float)) else str(v)


def report(results: dict) -> list[str]:
    runs = results["scenarios"]
    out = [f"# Profiling campaign {results['name']}", "",
           f"{results['exe']} ({results.get('commit', '')}), {len(runs)} scenarios, "
           f"{sum(1 for r in runs if r['status'] == 'ok')} ok. Times in ms a frame; spans on other threads add up "
           "beyond the frame.", "",
           "## Scenarios, slowest first", "",
           "| scenario | group | status | frame | worst period | slowest | slow frames | CPU | main | render | window wait | GPU passes | others' CPU |",
           "|---|---|---|---|---|---|---|---|---|---|---|---|---|"]
    for r in sorted(runs, key=lambda r: -(r["frame_mean"] or 0)):
        s = r["split"]
        out.append(f"| {r['name']} | {r['group']} | {r['status']} | {fmt(r['frame_mean'])} | {fmt(r['frame_worst_period'])} | "
                   f"{fmt(r['slowest_frame'])} | {len(r['hitches'])} | {fmt(r['cpu_ms'])} | {fmt(s.get('Main'))} | {fmt(s.get('Render'))} | "
                   f"{fmt(s.get('window::prepare_windows'))} | {fmt(r['gpu_total'])} | {fmt(r.get('others_cpu'))} |")
    # Spans across scenarios: the highest each reached, and in how many scenarios it is among the top ten.
    peak: dict[str, tuple[float, str]] = {}
    count: dict[str, int] = {}
    for r in runs:
        for i, (k, v) in enumerate(sorted(r["spans"].items(), key=lambda kv: -kv[1])):
            if v > peak.get(k, (0.0, ""))[0]:
                peak[k] = (v, r["name"])
            if i < 10:
                count[k] = count.get(k, 0) + 1
    out += ["", "## CPU spans across scenarios (highest first)", "",
            "| span | highest ms | in | top ten in |", "|---|---|---|---|"]
    for k, (v, where) in sorted(peak.items(), key=lambda kv: -kv[1][0])[:40]:
        out.append(f"| `{k}` | {v:.2f} | {where} | {count.get(k, 0)} |")
    gpeak: dict[str, tuple[float, str]] = {}
    for r in runs:
        for k, v in r["gpu"].items():
            if v > gpeak.get(k, (0.0, ""))[0]:
                gpeak[k] = (v, r["name"])
    out += ["", "## GPU passes across scenarios (highest first)", "", "| pass | highest ms | in |", "|---|---|---|"]
    for k, (v, where) in sorted(gpeak.items(), key=lambda kv: -kv[1][0])[:20]:
        out.append(f"| `{k}` | {v:.2f} | {where} |")
    hs = sorted(((h, r["name"]) for r in runs for h in r["hitches"]), key=lambda x: -x[0]["ms"])
    out += ["", f"## Slow frames (over each scenario's `slow_ms`, after the warm-up): {len(hs)}", "",
            "What grows in them, per scenario: each span's mean in its slow frames less its mean over every frame.", ""]
    for r in sorted(runs, key=lambda r: -len(r["hitches"])):
        if r["hitches"] and r.get("slow_frame_excess"):
            ex = ", ".join(f"`{k}` +{v:.1f}" for k, v in r["slow_frame_excess"].items() if v > 0)
            out.append(f"- **{r['name']}**: {len(r['hitches'])} slow frames; {ex}")
    out += ["", "The slowest of them:", ""]
    for h, name in hs[:25]:
        spans = ", ".join(f"{k} {v:.0f}" for k, v in h["spans"])
        out.append(f"- **{name}** at {fmt(h['t'])} s: {h['ms']:.0f} ms; {spans}"
                   + (f"; {h['new_pipelines']} new pipelines" if h["new_pipelines"] else ""))
    out += ["", "## Per scenario", ""]
    for r in runs:
        out += [f"### {r['name']} ({r['group']}, {r['status']})", "",
                f"`{r['command']}`", "",
                f"Frame {fmt(r['frame_mean'])} ms (worst period {fmt(r['frame_worst_period'])}, slowest frame "
                f"{fmt(r['slowest_frame'])}); CPU {fmt(r['cpu_ms'])} ms a frame over every thread; actors {fmt(r['actors'])}; "
                f"moved from {fmt(r['first_at'])} to {fmt(r['last_at'])}.", ""]
        if r["scene"]:
            sc = r["scene"]
            out.append(f"Scene: {sc['meshes']} mesh entities ({sc['visible']} visible, {sc['moved']} moved a frame), "
                       f"{sc['distinct_meshes']} meshes, {sc['materials']} materials; {fmt(r['entities_with_visibility'])} entities "
                       f"with visibility; transforms {fmt(r['transforms'])}; shadow casters {r['shadow_casters']}.")
            out.append("")
        out.append("CPU: " + ", ".join(f"`{k}` {v:.2f}" for k, v in list(r["spans"].items())[:15]))
        out.append("")
        out.append("GPU: " + ", ".join(f"`{k}` {v:.2f}" for k, v in list(r["gpu"].items())[:10]))
        out.append("")
    return out


# ---------------------------------------------------------------- running

def load(path: Path) -> tuple[dict, list[dict]]:
    with open(path, "rb") as f:
        doc = tomllib.load(f)
    d = doc.get("defaults", {})
    return d, doc.get("scenario", [])


def launch_args(d: dict, sc: dict) -> tuple[list[str], dict[str, str], float]:
    every = sc.get("every", d.get("every", 5))
    args = list(sc.get("args", [])) + ["--size", d.get("size", "1920x1080"), "--graphics", d.get("graphics", "enhanced"),
                                       "--no-voice", "--log-frame-times", f"{every:g}"]
    aids = {"NVF_PLAY_TEST_PROFILE": f"{every:g}", "NVF_PLAY_TEST_SAMPLE": f"{every:g}",
            "NVF_PLAY_TEST_PROFILE_HITCH_MS": f"{sc.get('slow_ms', d.get('slow_ms', 100)):g}",
            **{k: str(v) for k, v in sc.get("aids", {}).items()}}
    return args, aids, float(sc.get("seconds", d.get("seconds", 60)))


def run(args) -> int:
    from stress import battle, lib  # tools/stress: the launcher, the locks, the battle's layout

    d, scenarios = load(args.campaign)
    chosen = [s for s in scenarios if (not args.only or s["name"] in args.only) and (not args.group or s.get("group") in args.group)]
    if not chosen:
        print("no scenario chosen")
        return 2
    out = REPO / "target" / "profile-campaign" / (args.name or time.strftime("%Y%m%d-%H%M%S"))
    out.mkdir(parents=True, exist_ok=True)
    locks = None if args.no_locks else lib.Locks(lib.default_lock_paths(REPO))
    ctx = lib.Ctx(REPO, Path(args.exe), args.data, Path(), out, locks=locks)
    warmup = float(d.get("warmup", 15))
    results = {"name": out.name, "exe": str(args.exe), "commit": args.commit or "", "scenarios": []}
    old = json.loads((out / "results.json").read_text(encoding="utf-8")) if (out / "results.json").exists() else None
    done = {r["name"]: r for r in old["scenarios"]} if old else {}
    for i, sc in enumerate(chosen, 1):
        if sc["name"] in done and done[sc["name"]]["status"] == "ok":
            results["scenarios"].append(done[sc["name"]])
            continue
        a, aids, seconds = launch_args(d, sc)
        if b := sc.get("battle"):
            aid, player = battle.layout(int(b["per_side"]), int(b.get("third", 0)), battle.CENTRE, 3000.0)
            a = ["--cell", "Goodsprings", f"--pos={player[0]:g},{player[1]:g},9000", "--heading", "0", "--hour", "12"] + a
            aids["NVF_PLAY_TEST_BATTLE"] = aid
        play = sc.get("play", True)
        base = lib.BASE_ARGS
        if not play:
            # The free camera takes none of the play switches.
            lib.BASE_ARGS = [x for x in base if x not in ("--play", "--no-messages")]
            a = [x for x in a if x != "--no-voice"]
        try:
            print(f"[{i}/{len(chosen)}] {sc['name']} ({seconds:g} s)", flush=True)
            r = lib.run(ctx, lib.Launch(sc["name"], a, aids, run_seconds=seconds, progress=None, hang_seconds=180, memory_every=5))
        finally:
            lib.BASE_ARGS = base
        p = parse(r.lines, warmup)
        others = [c[2] for c in r.cpu if c[0] >= warmup]
        p.update({"name": sc["name"], "group": sc.get("group", ""), "status": r.status, "seconds": r.seconds,
                  "command": (out / f"{sc['name']}.cmd.txt").read_text(encoding="utf-8").strip().splitlines()[-1],
                  "peak_private_mb": round(max((m[2] for m in r.memory), default=0) / 2**20),
                  "others_cpu": round(statistics.mean(others), 3) if others else None})
        results["scenarios"].append(p)
        print(f"    {r.status}: frame {p['frame_mean']} ms, slowest {p['slowest_frame']} ms, {len(p['hitches'])} hitches", flush=True)
        (out / "results.json").write_text(json.dumps(results, indent=1), encoding="utf-8")
    (out / "results.json").write_text(json.dumps(results, indent=1), encoding="utf-8")
    (out / "report.md").write_text("\n".join(report(results)) + "\n", encoding="utf-8")
    print(f"report: {out / 'report.md'}")
    return 0


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(prog="python tools/profile/campaign.py", description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("run", help="run the scenarios")
    r.add_argument("--exe", required=True, help="a `profile` build of nvforever")
    r.add_argument("--data", required=True, help="the game's Data folder")
    r.add_argument("--campaign", type=Path, default=HERE / "campaign.toml")
    r.add_argument("--only", nargs="*", default=None, help="only these scenarios")
    r.add_argument("--group", nargs="*", default=None, help="only these groups (stand, crowd, interior, combat, move, stream)")
    r.add_argument("--name", default=None, help="the run's folder under target/profile-campaign/ (an existing one goes on where it stopped)")
    r.add_argument("--commit", default=None, help="the commit the exe was built from, for the report")
    r.add_argument("--no-locks", action="store_true", help="do not take the game locks")
    p = sub.add_parser("report", help="write report.md again from a run's results.json")
    p.add_argument("folder", type=Path)
    p.add_argument("--reparse", action="store_true", help="parse each scenario's saved log again first")
    p.add_argument("--campaign", type=Path, default=HERE / "campaign.toml")
    args = ap.parse_args(argv)
    if args.cmd == "run":
        return run(args)
    results = json.loads((args.folder / "results.json").read_text(encoding="utf-8"))
    if args.reparse:
        from stress.lib import clean  # noqa: F401  (tools/smoke's, through the stress package's path)
        d, _ = load(args.campaign)
        for i, r in enumerate(results["scenarios"]):
            log = args.folder / f"{r['name']}.log"
            if log.exists():
                lines = [clean(x) for x in log.read_text(encoding="utf-8", errors="replace").splitlines()]
                keep = {k: r[k] for k in ("name", "group", "status", "seconds", "command", "peak_private_mb", "others_cpu") if k in r}
                results["scenarios"][i] = {**parse(lines, float(d.get("warmup", 15))), **keep}
        (args.folder / "results.json").write_text(json.dumps(results, indent=1), encoding="utf-8")
    (args.folder / "report.md").write_text("\n".join(report(results)) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
